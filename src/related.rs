use crate::jira::Issue;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};

pub struct Group {
    pub id: String,
    pub title: String,
    pub rows: Vec<Issue>,
    seen: HashSet<String>,
}

#[derive(Default)]
pub struct Navigation {
    pub root: String,
    pub query: String,
    target: Option<(String, String)>,
}

impl Navigation {
    pub fn new(root: String) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }

    pub fn filter(&self, mut groups: Vec<Group>) -> Vec<Group> {
        let query = self.query.to_lowercase();
        for group in &mut groups {
            group.rows.retain(|issue| {
                let text = format!(
                    "{} {} {} {} {} {}",
                    group.title,
                    issue.key,
                    issue.field("summary"),
                    issue.field("assignee"),
                    issue.field("status"),
                    issue.field("priority")
                )
                .to_lowercase();
                query.split_whitespace().all(|word| text.contains(word))
            });
        }
        groups.retain(|group| !group.rows.is_empty());
        groups
    }

    pub fn selected(&self, groups: &[Group]) -> Option<(String, String)> {
        let mut targets = groups.iter().flat_map(|group| {
            group
                .rows
                .iter()
                .map(move |issue| (group.id.clone(), issue.key.clone()))
        });
        self.target
            .as_ref()
            .filter(|target| targets.clone().any(|row| &row == *target))
            .cloned()
            .or_else(|| targets.next())
    }

    pub fn move_selection(&mut self, groups: &[Group], down: bool) {
        let targets: Vec<_> = groups
            .iter()
            .flat_map(|group| {
                group
                    .rows
                    .iter()
                    .map(move |issue| (group.id.clone(), issue.key.clone()))
            })
            .collect();
        let current = self.selected(groups);
        let index = targets
            .iter()
            .position(|row| Some(row) == current.as_ref())
            .unwrap_or(0);
        let index = if down {
            (index + 1).min(targets.len().saturating_sub(1))
        } else {
            index.saturating_sub(1)
        };
        self.target = targets.get(index).cloned();
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.target = None;
    }
}

pub fn valid_key(key: &str) -> bool {
    key.rsplit_once('-').is_some_and(|(project, number)| {
        !project.is_empty()
            && project
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            && !number.is_empty()
            && number.bytes().all(|c| c.is_ascii_digit())
    })
}

fn reference(value: &Value) -> Option<Issue> {
    let issue: Issue = serde_json::from_value(value.clone()).ok()?;
    valid_key(&issue.key).then_some(issue)
}

pub fn references(issue: &Issue) -> Vec<Issue> {
    std::iter::once(&issue.fields["parent"])
        .chain(issue.fields["subtasks"].as_array().into_iter().flatten())
        .chain(
            issue.fields["issuelinks"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|link| [&link["inwardIssue"], &link["outwardIssue"]]),
        )
        .filter_map(reference)
        .filter(|related| related.key != issue.key)
        .collect()
}

pub fn query(issue: &Issue) -> anyhow::Result<String> {
    anyhow::ensure!(valid_key(&issue.key), "Invalid issue key");
    let keys: BTreeSet<_> = references(issue)
        .into_iter()
        .map(|issue| issue.key)
        .collect();
    let mut query = format!("parent = \"{}\"", issue.key);
    if !keys.is_empty() {
        query.push_str(&format!(
            " OR key IN ({})",
            keys.iter()
                .map(|key| format!("\"{key}\""))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    query.push_str(" ORDER BY key ASC");
    Ok(query)
}

fn resolve(mut issue: Issue, known: &HashMap<String, Issue>) -> Issue {
    if let Some(cached) = known.get(&issue.key) {
        let mut fields = issue.fields.as_object().cloned().unwrap_or_default();
        for field in [
            "summary",
            "status",
            "priority",
            "assignee",
            "issuetype",
            "parent",
        ] {
            if let Some(value) = cached.fields.get(field) {
                fields.insert(field.into(), value.clone());
            }
        }
        issue.fields = Value::Object(fields);
    }
    issue
}

fn push(group: &mut Group, issue: Issue, root: &str) {
    if issue.key != root && group.seen.insert(issue.key.clone()) {
        group.rows.push(issue);
    }
}

pub fn groups(issue: &Issue, known: &HashMap<String, Issue>, children: &[String]) -> Vec<Group> {
    let mut groups = Vec::new();
    if let Some(parent) = reference(&issue.fields["parent"])
        && parent.key != issue.key
    {
        groups.push(Group {
            id: "parent".into(),
            title: "Parent".into(),
            seen: HashSet::from([parent.key.clone()]),
            rows: vec![resolve(parent, known)],
        });
    }
    let embedded: Vec<_> = issue.fields["subtasks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(reference)
        .collect();
    let embedded_keys: HashSet<_> = embedded.iter().map(|row| row.key.as_str()).collect();
    let mut child_group = Group {
        id: "children".into(),
        title: if issue.field("issuetype").eq_ignore_ascii_case("epic") {
            "Issues in this epic"
        } else {
            "Child issues"
        }
        .into(),
        rows: Vec::new(),
        seen: HashSet::new(),
    };
    let mut subtasks = Group {
        id: "subtasks".into(),
        title: "Subtasks".into(),
        rows: Vec::new(),
        seen: HashSet::new(),
    };
    for child in embedded
        .iter()
        .cloned()
        .chain(
            children
                .iter()
                .filter(|key| valid_key(key))
                .map(|key| Issue {
                    key: key.clone(),
                    fields: json!({}),
                }),
        )
    {
        let child = resolve(child, known);
        let subtask = embedded_keys.contains(child.key.as_str())
            || child.fields["issuetype"]["subtask"] == true;
        push(
            if subtask {
                &mut subtasks
            } else {
                &mut child_group
            },
            child,
            &issue.key,
        );
    }
    groups.extend(
        [child_group, subtasks]
            .into_iter()
            .filter(|group| !group.rows.is_empty()),
    );
    let mut links: Vec<Group> = Vec::new();
    for link in issue.fields["issuelinks"].as_array().into_iter().flatten() {
        for direction in ["inward", "outward"] {
            if let Some(target) = reference(&link[format!("{direction}Issue")]) {
                let label = link["type"][direction]
                    .as_str()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| link["type"]["name"].as_str())
                    .unwrap_or("Related")
                    .trim();
                let id = format!("link:{}", label.to_lowercase());
                let index = links
                    .iter()
                    .position(|group| group.id == id)
                    .unwrap_or_else(|| {
                        let title = if label.eq_ignore_ascii_case("is blocked by") {
                            "Blocked by".into()
                        } else {
                            let mut chars = label.chars();
                            chars
                                .next()
                                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                                .unwrap_or_else(|| "Related".into())
                        };
                        links.push(Group {
                            id: id.clone(),
                            title,
                            rows: Vec::new(),
                            seen: HashSet::new(),
                        });
                        links.len() - 1
                    });
                push(&mut links[index], resolve(target, known), &issue.key);
            }
        }
    }
    links.sort_by_key(|group| {
        (
            match group.title.to_ascii_lowercase().as_str() {
                "blocked by" => 0,
                "blocks" => 1,
                _ => 2,
            },
            group.title.clone(),
        )
    });
    groups.extend(links.into_iter().filter(|group| !group.rows.is_empty()));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    fn issue(key: &str, fields: Value) -> Issue {
        Issue {
            key: key.into(),
            fields,
        }
    }

    #[test]
    fn navigation_crosses_groups_and_filters_live_without_losing_link_identity() {
        let root = issue(
            "P-1",
            json!({"parent":{"key":"P-9","fields":{"summary":"Epic"}},
            "subtasks":[{"key":"P-2","fields":{"summary":"Keyboard navigation"}}],
            "issuelinks":[{"type":{"inward":"is blocked by"},"inwardIssue":{"key":"P-2","fields":{"summary":"Keyboard navigation"}}}]}),
        );
        let all = || groups(&root, &HashMap::new(), &[]);
        let mut nav = Navigation::new(root.key.clone());
        assert_eq!(nav.selected(&all()), Some(("parent".into(), "P-9".into())));
        nav.move_selection(&all(), true);
        assert_eq!(
            nav.selected(&all()),
            Some(("subtasks".into(), "P-2".into()))
        );
        nav.move_selection(&all(), true);
        assert_eq!(
            nav.selected(&all()),
            Some(("link:is blocked by".into(), "P-2".into()))
        );
        nav.move_selection(&all(), true);
        assert_eq!(nav.selected(&all()).unwrap().0, "link:is blocked by");
        nav.set_query("KEYboard p-2".into());
        let visible = nav.filter(all());
        assert_eq!(visible.len(), 2);
        assert_eq!(nav.selected(&visible).unwrap().0, "subtasks");
        nav.set_query("blocked keyboard".into());
        assert_eq!(
            nav.selected(&nav.filter(all())).unwrap().0,
            "link:is blocked by"
        );
        nav.set_query("unmatched".into());
        let empty = nav.filter(all());
        nav.move_selection(&empty, true);
        assert!(nav.selected(&empty).is_none());
        nav.set_query(String::new());
        assert_eq!(nav.selected(&nav.filter(all())).unwrap().1, "P-9");
    }

    #[test]
    fn navigation_handles_late_results_and_removed_selection() {
        let root = issue("P-1", json!({"issuetype":{"name":"Epic"}}));
        let known = HashMap::from([
            (
                "P-2".into(),
                issue("P-2", json!({"summary":"Jira keyboard work"})),
            ),
            ("P-3".into(), issue("P-3", json!({"summary":"Caching"}))),
        ]);
        let mut nav = Navigation::new(root.key.clone());
        nav.set_query("jira".into());
        assert!(
            nav.selected(&nav.filter(groups(&root, &known, &[])))
                .is_none()
        );
        let loaded = nav.filter(groups(&root, &known, &["P-2".into(), "P-3".into()]));
        assert_eq!(nav.selected(&loaded).unwrap().1, "P-2");
        nav.set_query(String::new());
        let all = nav.filter(groups(&root, &known, &["P-2".into(), "P-3".into()]));
        nav.move_selection(&all, true);
        assert_eq!(nav.selected(&all).unwrap().1, "P-3");
        assert_eq!(
            nav.selected(&groups(&root, &known, &["P-2".into()]))
                .unwrap()
                .1,
            "P-2"
        );
    }

    #[test]
    fn preserves_direction_custom_names_and_distinct_relationships() {
        let root = issue(
            "P-1",
            json!({"issuelinks":[
                {"type":{"inward":"is blocked by","outward":"blocks"},"inwardIssue":{"key":"P-2"}},
                {"type":{"inward":"is blocked by","outward":"blocks"},"outwardIssue":{"key":"P-3"}},
                {"type":{"inward":"is blocked by","outward":"blocks"},"inwardIssue":{"key":"P-2"}},
                {"type":{"outward":"validates delivery"},"outwardIssue":{"key":"P-2"}}
            ]}),
        );
        let groups = groups(&root, &HashMap::new(), &[]);
        assert_eq!(
            groups
                .iter()
                .map(|group| group.title.as_str())
                .collect::<Vec<_>>(),
            ["Blocked by", "Blocks", "Validates delivery"]
        );
        assert_eq!(groups[0].rows.len(), 1);
        assert_eq!(groups[0].rows[0].key, "P-2");
        assert_eq!(groups[1].rows[0].key, "P-3");
        assert_eq!(groups[2].rows[0].key, "P-2");
    }

    #[test]
    fn separates_parent_epic_children_and_subtasks_without_duplicates() {
        let root = issue(
            "P-1",
            json!({"issuetype":{"name":"Epic"},"parent":{"key":"P-9"},"subtasks":[{"key":"P-3"}]}),
        );
        let known = HashMap::from([
            (
                "P-2".into(),
                issue(
                    "P-2",
                    json!({"summary":"Story","issuetype":{"subtask":false}}),
                ),
            ),
            (
                "P-3".into(),
                issue(
                    "P-3",
                    json!({"summary":"Subtask","issuetype":{"subtask":true}}),
                ),
            ),
        ]);
        let groups = groups(&root, &known, &["P-2".into(), "P-3".into()]);
        assert_eq!(
            groups.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(),
            ["Parent", "Issues in this epic", "Subtasks"]
        );
        assert_eq!(groups[1].rows[0].field("summary"), "Story");
        assert_eq!(groups[2].rows.len(), 1);
    }

    #[test]
    fn safe_query_includes_direct_children_and_unique_reference_keys() {
        let root = issue(
            "P-1",
            json!({"parent":{"key":"P-9"},"subtasks":[{"key":"P-2"},{"key":"P-2"},{"key":"P-1"},{"key":"P-2\" OR assignee=currentUser()"}]}),
        );
        assert_eq!(
            query(&root).unwrap(),
            "parent = \"P-1\" OR key IN (\"P-2\",\"P-9\") ORDER BY key ASC"
        );
        assert!(query(&issue("P-1\" OR project=X", json!({}))).is_err());
        assert!(
            groups(
                &issue("P-1", json!({"parent":{"key":"P-1"}})),
                &HashMap::new(),
                &[]
            )
            .is_empty()
        );
    }
}
