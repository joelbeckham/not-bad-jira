use crate::{
    config::{Config, Tab},
    jira::Issue,
    store::Store,
};

#[derive(Clone)]
pub enum Change {
    Add(Tab, Option<usize>),
    Replace(usize, Tab),
    Delete(usize),
}

impl Tab {
    pub fn query(&self) -> String {
        if self.filter_id.is_empty() {
            self.jql.clone()
        } else {
            format!("filter = {}", self.filter_id)
        }
    }
    pub fn with_query(label: String, jql: String) -> Self {
        Self {
            label,
            jql,
            columns: ["key", "summary", "status", "assignee", "priority"]
                .map(String::from)
                .to_vec(),
            ..Default::default()
        }
    }
    pub fn for_epic(issue: &Issue) -> anyhow::Result<Self> {
        anyhow::ensure!(
            is_epic(issue) && crate::related::valid_key(&issue.key),
            "Select an epic"
        );
        Ok(Self::with_query(
            issue.key.clone(),
            format!("parent = \"{}\" ORDER BY Rank ASC", issue.key),
        ))
    }
}

pub fn is_epic(issue: &Issue) -> bool {
    issue.fields["issuetype"]["hierarchyLevel"].as_i64() == Some(1)
        || issue.field("issuetype").eq_ignore_ascii_case("epic")
}

impl Change {
    pub fn config(&self, current: &Config) -> anyhow::Result<Config> {
        let mut next = current.clone();
        match self {
            Self::Add(tab, _) => next.tabs.push(tab.clone()),
            Self::Replace(index, tab) => {
                *next
                    .tabs
                    .get_mut(*index)
                    .ok_or_else(|| anyhow::anyhow!("Tab no longer exists"))? = tab.clone();
            }
            Self::Delete(index) => {
                anyhow::ensure!(*index < next.tabs.len(), "Tab no longer exists");
                next.tabs.remove(*index);
            }
        }
        next.validate()?;
        Ok(next)
    }

    // Apply only after persistence succeeds, using the freshest issue data.
    pub fn apply(&self, old: &Config, store: &mut Store, active: usize) -> usize {
        store.tabs.resize_with(old.tabs.len(), Vec::new);
        match self {
            Self::Add(_, source) => {
                let rows = source
                    .and_then(|i| store.tabs.get(i))
                    .cloned()
                    .unwrap_or_default();
                store.tabs.push(rows);
                old.tabs.len()
            }
            Self::Replace(index, tab) => {
                if tab.query() != old.tabs[*index].query() {
                    store.tabs[*index].clear();
                }
                active
            }
            Self::Delete(index) => {
                store.tabs.remove(*index);
                if active > *index {
                    active - 1
                } else {
                    active.min(store.tabs.len().saturating_sub(1))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config::initial(
            "https://example.atlassian.net".into(),
            "a@example.com".into(),
            "T".into(),
        )
    }
    #[test]
    fn delete_remaps_rows_and_allows_last_tab() {
        let mut c = config();
        c.tabs
            .push(Tab::with_query("Two".into(), "project=T".into()));
        let mut store = Store {
            tabs: vec![vec!["T-1".into()], vec!["T-2".into()]],
            ..Default::default()
        };
        let delete = Change::Delete(0);
        assert_eq!(delete.apply(&c, &mut store, 1), 0);
        assert_eq!(store.tabs, vec![vec!["T-2"]]);
        c = delete.config(&c).unwrap();
        assert_eq!(delete.apply(&c, &mut store, 0), 0);
        assert!(delete.config(&c).unwrap().tabs.is_empty());
        assert!(store.tabs.is_empty());
    }
    #[test]
    fn rename_and_duplicate_preserve_filter_columns_and_rows() {
        let mut c = config();
        c.tabs[0].jql.clear();
        c.tabs[0].filter_id = "42".into();
        c.tabs[0].sort = "updated".into();
        let mut store = Store {
            tabs: vec![vec!["T-1".into()]],
            ..Default::default()
        };
        let mut tab = c.tabs[0].clone();
        tab.label = "Renamed".into();
        let rename = Change::Replace(0, tab.clone());
        rename.apply(&c, &mut store, 0);
        c = rename.config(&c).unwrap();
        let duplicate = Change::Add(tab, Some(0));
        assert_eq!(duplicate.apply(&c, &mut store, 0), 1);
        let next = duplicate.config(&c).unwrap();
        assert_eq!(next.tabs[0], next.tabs[1]);
        assert_eq!(store.tabs[0], store.tabs[1]);
        let edited = Change::Replace(0, Tab::with_query("Changed".into(), "project=T".into()));
        edited.apply(&next, &mut store, 1);
        assert!(store.tabs[0].is_empty());
        assert_eq!(store.tabs[1], ["T-1"]);
    }
    #[test]
    fn epic_query_excludes_related_links_and_validates_key() {
        let mut issue = Issue {
            key: "T-123".into(),
            fields: serde_json::json!({"issuetype":{"name":"Initiative", "hierarchyLevel":1}}),
        };
        assert_eq!(
            Tab::for_epic(&issue).unwrap().jql,
            "parent = \"T-123\" ORDER BY Rank ASC"
        );
        issue.key = "T-123\" OR project=X".into();
        assert!(Tab::for_epic(&issue).is_err());
    }
}
