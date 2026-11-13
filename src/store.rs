use crate::jira::Issue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Store {
    pub issues: HashMap<String, Issue>,
    pub tabs: Vec<Vec<String>>,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    site: String,
    tabs: Vec<String>,
    store: Store,
}
pub fn writer() -> std::sync::mpsc::Sender<(Store, crate::config::Config)> {
    let (tx, rx) = std::sync::mpsc::channel::<(Store, crate::config::Config)>();
    std::thread::spawn(move || {
        while let Ok((store, c)) = rx.recv() {
            let _ = store.save(&c);
        }
    });
    tx
}
impl Store {
    pub fn load(c: &crate::config::Config) -> Self {
        let result = (|| -> anyhow::Result<Self> {
            let p = crate::config::cache_path();
            if std::fs::metadata(&p)?.len() > 8 * 1024 * 1024 {
                anyhow::bail!("Cache exceeds limit");
            }
            let cache: Cache = serde_json::from_slice(&std::fs::read(p)?)?;
            if cache.version != 1
                || cache.site != format!("{}|{}", c.jira.base_url, c.jira.email)
                || cache.tabs != signatures(c)
            {
                anyhow::bail!("Cache mismatch");
            }
            Ok(cache.store)
        })();
        result.unwrap_or_default()
    }
    pub fn save(&self, c: &crate::config::Config) -> anyhow::Result<()> {
        let p = crate::config::cache_path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let cache = serde_json::json!({"version":1,"site":format!("{}|{}", c.jira.base_url,c.jira.email),"tabs":signatures(c),"store":self});
        let tmp = p.with_extension("tmp");
        let bytes = serde_json::to_vec(&cache)?;
        if bytes.len() > 8 * 1024 * 1024 {
            anyhow::bail!("Cache exceeds 8 MiB limit");
        }
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, p)?;
        Ok(())
    }
    pub fn set_tab(&mut self, index: usize, issues: Vec<Issue>) {
        self.tabs
            .resize_with(self.tabs.len().max(index + 1), Vec::new);
        self.tabs[index] = issues.iter().map(|i| i.key.clone()).collect();
        for issue in issues {
            if let Some(existing) = self.issues.get_mut(&issue.key) {
                if let (Some(old), Some(new)) =
                    (existing.fields.as_object_mut(), issue.fields.as_object())
                {
                    for (k, v) in new {
                        old.insert(k.clone(), v.clone());
                    }
                }
            } else {
                self.issues.insert(issue.key.clone(), issue);
            }
        }
    }
}
fn signatures(c: &crate::config::Config) -> Vec<String> {
    c.tabs
        .iter()
        .map(|t| format!("{}|{}", t.jql, t.filter_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_refresh_preserves_detail_fields() {
        let mut store = Store::default();
        store.issues.insert("T-1".into(),Issue{key:"T-1".into(),fields:serde_json::json!({"summary":"Old","description":{"type":"doc","version":1,"content":[]},"comment":{"comments":[{"id":"1"}]}})});
        store.set_tab(
            0,
            vec![Issue {
                key: "T-1".into(),
                fields: serde_json::json!({"summary":"New"}),
            }],
        );
        assert_eq!(store.issues["T-1"].field("summary"), "New");
        assert_eq!(store.issues["T-1"].fields["description"]["type"], "doc");
        assert_eq!(
            store.issues["T-1"].fields["comment"]["comments"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
