use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub value: Value,
}

impl Choice {
    pub fn from_jira(kind: &str, data: &Value) -> Vec<Self> {
        data.as_array()
            .into_iter()
            .flatten()
            .filter(|v| kind != "done" || v["to"]["statusCategory"]["key"] == "done")
            .filter_map(|v| {
                let value = if kind == "status" || kind == "done" {
                    &v["to"]
                } else {
                    v
                };
                Some(Self {
                    id: v[if kind == "assignee" {
                        "accountId"
                    } else {
                        "id"
                    }]
                    .as_str()?
                    .into(),
                    label: value[if kind == "assignee" {
                        "displayName"
                    } else {
                        "name"
                    }]
                    .as_str()
                    .or_else(|| v["name"].as_str())?
                    .into(),
                    value: value.clone(),
                })
            })
            .collect()
    }

    pub fn named(id: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: json!({"name": label}),
        }
    }
}

pub struct Picker {
    pub kind: String,
    pub key: String,
    pub request: u64,
    pub choices: Vec<Choice>,
    pub visible: Vec<usize>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    query: String,
    current_label: Option<String>,
    current_value: Value,
}

impl Picker {
    pub fn new(kind: &str, key: String, request: u64) -> Self {
        Self {
            kind: kind.into(),
            key,
            request,
            choices: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            loading: true,
            error: None,
            query: String::new(),
            current_label: None,
            current_value: Value::Null,
        }
    }

    pub fn set_choices(&mut self, mut choices: Vec<Choice>) {
        // Jira transitions usually omit the current status. Keep it available as
        // a no-op so opening the picker and pressing Enter never changes it.
        if let Some(label) = &self.current_label
            && !choices.iter().any(|choice| self.is_current(choice))
        {
            choices.insert(
                0,
                Choice {
                    id: String::new(),
                    label: label.clone(),
                    value: self.current_value.clone(),
                },
            );
        }
        self.choices = choices;
        self.loading = false;
        self.filter();
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.filter();
    }

    pub fn set_current(&mut self, label: String, value: Value) {
        self.current_label = Some(label);
        self.current_value = value;
        self.filter();
    }

    pub fn is_current(&self, choice: &Choice) -> bool {
        self.current_label
            .as_ref()
            .is_some_and(|label| label == &choice.label)
    }

    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        self.visible = self
            .choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| {
                let label = choice.label.to_lowercase();
                query.split_whitespace().all(|word| label.contains(word))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = if query.trim().is_empty() {
            self.visible
                .iter()
                .position(|index| self.is_current(&self.choices[*index]))
                .unwrap_or(0)
        } else {
            0
        };
    }

    pub fn move_selection(&mut self, down: bool) {
        self.selected = if down {
            (self.selected + 1).min(self.visible.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
    }

    pub fn current(&self) -> Option<&Choice> {
        if self.loading || self.error.is_some() {
            return None;
        }
        self.visible
            .get(self.selected)
            .and_then(|index| self.choices.get(*index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_selects_filtered_choices_and_handles_no_matches() {
        let mut picker = Picker::new("assignee", "TEST-1".into(), 1);
        picker.set_choices(vec![
            Choice::named("1", "Alex Morgan"),
            Choice::named("2", "Sam Rivera"),
            Choice::named("3", "Alex Rivera"),
        ]);
        picker.set_query("RIVERA".into());
        assert_eq!(picker.current().unwrap().id, "2");
        picker.move_selection(true);
        assert_eq!(picker.current().unwrap().id, "3");
        picker.move_selection(true);
        assert_eq!(picker.current().unwrap().id, "3");
        picker.move_selection(false);
        assert_eq!(picker.current().unwrap().id, "2");
        picker.set_query("alex rivera".into());
        assert_eq!(picker.current().unwrap().id, "3");
        picker.set_query("nobody".into());
        picker.move_selection(true);
        assert!(picker.current().is_none());
    }

    #[test]
    fn late_choices_respect_text_entered_while_loading() {
        let mut picker = Picker::new("priority", "TEST-1".into(), 2);
        picker.set_query("high".into());
        assert!(picker.current().is_none());
        picker.set_choices(vec![Choice::named("1", "Low"), Choice::named("2", "High")]);
        assert_eq!(picker.current().unwrap().id, "2");
    }

    #[test]
    fn opening_and_clearing_filter_preserve_the_current_value() {
        let mut picker = Picker::new("priority", "TEST-1".into(), 1);
        picker.set_current("High".into(), json!({"name":"High"}));
        picker.set_choices(vec![
            Choice::named("1", "Highest"),
            Choice::named("2", "High"),
            Choice::named("3", "Low"),
        ]);
        assert_eq!(picker.current().unwrap().id, "2");
        assert!(picker.is_current(picker.current().unwrap()));
        picker.set_query("high".into());
        assert_eq!(picker.current().unwrap().id, "1");
        picker.set_query(String::new());
        assert_eq!(picker.current().unwrap().id, "2");
    }

    #[test]
    fn transitions_keep_action_id_and_target_status_category() {
        let choices = Choice::from_jira(
            "status",
            &json!([{"id":"21","name":"Start work","to":{"id":"3","name":"Engineering review","statusCategory":{"key":"indeterminate"}}}]),
        );
        assert_eq!(choices[0].id, "21");
        assert_eq!(choices[0].label, "Engineering review");
        assert_eq!(choices[0].value["statusCategory"]["key"], "indeterminate");
        assert_eq!(choices[0].value["id"], "3");
    }

    #[test]
    fn missing_current_transition_remains_a_noop_with_its_icon_metadata() {
        let mut picker = Picker::new("status", "TEST-1".into(), 1);
        picker.set_current(
            "In Progress".into(),
            json!({"name":"In Progress","statusCategory":{"key":"indeterminate"}}),
        );
        picker.set_choices(Choice::from_jira("status", &json!([{"id":"31","name":"Finish","to":{"name":"Done","statusCategory":{"key":"done"}}}])));
        let current = picker.current().unwrap();
        assert_eq!(current.label, "In Progress");
        assert!(current.id.is_empty());
        assert_eq!(current.value["statusCategory"]["key"], "indeterminate");
        picker.move_selection(true);
        assert_eq!(picker.current().unwrap().id, "31");
    }
}
