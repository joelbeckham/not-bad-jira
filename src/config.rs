use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Config {
    pub jira: Jira,
    pub tabs: Vec<Tab>,
    #[serde(default = "ttl")]
    pub cache_ttl: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Jira {
    pub base_url: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub default_project: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Tab {
    pub label: String,
    #[serde(default)]
    pub filter_id: String,
    #[serde(default)]
    pub jql: String,
    #[serde(default = "columns")]
    pub columns: Vec<String>,
    #[serde(default)]
    pub sort: String,
}
fn ttl() -> String {
    "5m".into()
}
fn columns() -> Vec<String> {
    ["key", "summary", "status", "assignee", "priority"]
        .map(String::from)
        .to_vec()
}
pub fn path() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("not-bad-jira/config.yaml")
}
pub fn cache_path() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("not-bad-jira/cache/issues.json")
}
impl Config {
    pub fn load() -> Result<Self> {
        let c: Self = serde_norway::from_str(&std::fs::read_to_string(path())?)?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<()> {
        let url = url::Url::parse(&self.jira.base_url)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            bail!("Enter an HTTPS Jira site URL without a path");
        }
        self.ttl_seconds()?;
        if !self.jira.email.contains('@') {
            bail!("Enter your Jira account email");
        }
        if self.tabs.is_empty() {
            bail!("Configure at least one tab");
        }
        for t in &self.tabs {
            if t.label.is_empty() || t.jql.is_empty() == t.filter_id.is_empty() {
                bail!("Each tab needs a label and exactly one of jql or filter_id");
            }
            if t.columns.is_empty()
                || t.columns.iter().any(|c| {
                    ![
                        "key", "summary", "status", "priority", "assignee", "reporter", "type",
                        "project", "created", "updated", "duedate",
                    ]
                    .contains(&c.as_str())
                })
            {
                bail!("Tab {} has empty or unsupported columns", t.label);
            }
            if !t.filter_id.is_empty() && !t.filter_id.chars().all(|c| c.is_ascii_digit()) {
                bail!("Filter IDs must contain only digits");
            }
        }
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let p = path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, serde_norway::to_string(self)?)?;
        Ok(())
    }
    pub fn ttl_seconds(&self) -> Result<u64> {
        let s = self.cache_ttl.trim();
        let unit = s.chars().last().unwrap_or('m');
        let factor = match unit {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            _ => bail!("cache_ttl must use s, m, or h"),
        };
        let n: u64 = s.strip_suffix(unit).unwrap_or("").parse()?;
        if n == 0 {
            bail!("cache_ttl must be positive");
        }
        n.checked_mul(factor)
            .ok_or_else(|| anyhow::anyhow!("cache_ttl is too large"))
    }
    pub fn credential(&self) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(
            "not-bad-jira",
            &format!(
                "{}|{}",
                self.jira.base_url.trim_end_matches('/'),
                self.jira.email
            ),
        )?)
    }
    pub fn initial(base_url: String, email: String, default_project: String) -> Self {
        Self {
            jira: Jira {
                base_url: base_url.trim_end_matches('/').into(),
                email,
                default_project,
            },
            tabs: vec![Tab {
                label: "My issues".into(),
                jql: "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"
                    .into(),
                columns: columns(),
                ..Default::default()
            }],
            cache_ttl: ttl(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_due_tab_after_config_roundtrip() {
        let mut config = Config::initial(
            "https://example.atlassian.net".into(),
            "test@example.com".into(),
            "TEST".into(),
        );
        config.tabs[0].label = "Due".into();
        config.tabs[0].columns = ["duedate", "priority", "assignee", "summary", "status"]
            .map(String::from)
            .to_vec();
        let yaml = serde_norway::to_string(&config).unwrap();
        let loaded: Config = serde_norway::from_str(&yaml).unwrap();
        loaded
            .validate()
            .expect("Due-date columns must remain supported");
    }
    #[test]
    fn rejects_ambiguous_tab() {
        let mut c = Config::initial(
            "https://example.atlassian.net".into(),
            "a@example.com".into(),
            "TEST".into(),
        );
        c.tabs[0].filter_id = "42".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_bad_columns() {
        let mut c = Config::initial(
            "https://example.atlassian.net".into(),
            "a@example.com".into(),
            "TEST".into(),
        );
        c.tabs[0].columns = vec!["invalid".into()];
        assert!(c.validate().is_err());
    }
    #[test]
    fn serialization_excludes_token() {
        let c = Config::initial(
            "https://example.atlassian.net".into(),
            "a@example.com".into(),
            "TEST".into(),
        );
        assert!(c.validate().is_ok());
        let yaml = serde_norway::to_string(&c).unwrap();
        assert!(!yaml.contains("token"));
        let restored: Config = serde_norway::from_str(&yaml).unwrap();
        assert_eq!(restored.tabs[0].jql, c.tabs[0].jql);
    }
}
