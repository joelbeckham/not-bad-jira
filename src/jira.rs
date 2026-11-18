use crate::config::Config;
use anyhow::{Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    #[serde(default)]
    pub fields: Value,
}
pub struct CreatedIssue {
    pub issue: Issue,
    pub detail_error: Option<String>,
}
pub struct ParentDetails {
    pub issue: Issue,
    pub color: Option<String>,
}
impl Issue {
    pub fn list_field(&self, name: &str) -> String {
        let value = self.field(name);
        if matches!(name, "created" | "updated" | "duedate") {
            // Keep Jira's calendar date, including date-only due dates.
            if let Some(date) = value
                .get(..10)
                .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            {
                return date.format("%b %-d, %Y").to_string();
            }
        }
        value
    }

    pub fn field(&self, name: &str) -> String {
        let v = &self.fields[name];
        v.as_str()
            .or_else(|| v["displayName"].as_str())
            .or_else(|| v["name"].as_str())
            .unwrap_or("—")
            .to_string()
    }
}
#[derive(Clone)]
pub struct Client {
    pub config: Config,
    auth: String,
    agent: ureq::Agent,
    parent_color_fields: Arc<OnceLock<[Option<String>; 2]>>,
}
impl Client {
    pub fn new(config: Config, token: String) -> Self {
        let auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", config.jira.email, token))
        );
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .into();
        Self {
            config,
            auth,
            agent,
            parent_color_fields: Default::default(),
        }
    }
    fn url(&self, path: &str) -> String {
        format!("{}/rest/api/3/{}", self.config.jira.base_url, path)
    }
    pub fn get(&self, path: &str) -> Result<Value> {
        self.get_json(&self.url(path))
    }

    fn get_json(&self, address: &str) -> Result<Value> {
        Ok(checked(
            self.agent
                .get(address)
                .header("Authorization", &self.auth)
                .call()?,
        )?
        .body_mut()
        .read_json()?)
    }

    pub fn parent_details(&self, keys: &[String]) -> Result<Vec<ParentDetails>> {
        anyhow::ensure!(
            keys.iter().all(|key| crate::related::valid_key(key)),
            "Invalid parent key"
        );
        if self.parent_color_fields.get().is_none()
            && let Ok(fields) = self.get("field")
            && let Some(fields) = fields.as_array()
        {
            let ids = ["Issue color", "Epic Color"].map(|name| {
                fields.iter().find_map(|field| {
                    let id = field["id"].as_str()?;
                    let field_name = field["name"].as_str()?;
                    (field_name.eq_ignore_ascii_case(name) && id.starts_with("customfield_"))
                        .then(|| id.to_owned())
                })
            });
            let _ = self.parent_color_fields.set(ids);
        }
        let color_fields = self
            .parent_color_fields
            .get()
            .into_iter()
            .flatten()
            .flatten();
        let fields: Vec<_> = ["summary", "issuetype"]
            .into_iter()
            .chain(color_fields.clone().map(String::as_str))
            .collect();
        let query = format!(
            "key in ({})",
            keys.iter()
                .map(|key| format!("\"{key}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        let issues = self.search_fields(&query, &fields)?;
        Ok(issues
            .into_iter()
            .filter(|issue| keys.contains(&issue.key))
            .map(|issue| {
                let color = if crate::tabs::is_epic(&issue) {
                    color_fields
                        .clone()
                        .find_map(|field| {
                            let color = &issue.fields[field];
                            color
                                .as_str()
                                .or_else(|| color["key"].as_str())
                                .or_else(|| color["value"].as_str())
                                .filter(|color| !color.trim().is_empty())
                                .map(str::to_owned)
                        })
                        .or_else(|| {
                            let site = self.config.jira.base_url.trim_end_matches('/');
                            let data = self
                                .get_json(&format!("{site}/rest/agile/1.0/epic/{}", issue.key))
                                .ok()?;
                            data["color"]["key"].as_str().map(str::to_owned)
                        })
                } else {
                    None
                };
                ParentDetails { issue, color }
            })
            .collect())
    }

    pub fn avatar_bytes(&self, address: &str) -> Result<Vec<u8>> {
        self.image_bytes(address, 1024 * 1024)
    }
    pub fn attachment_preview(&self, id: &str) -> Result<Vec<u8>> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            bail!("Invalid attachment ID");
        }
        self.image_bytes(
            &self.url(&format!("attachment/content/{id}")),
            8 * 1024 * 1024,
        )
    }
    fn image_bytes(&self, address: &str, limit: u64) -> Result<Vec<u8>> {
        let site = url::Url::parse(&self.config.jira.base_url)?;
        let mut address = url::Url::parse(address)?;
        // Resolve redirects explicitly: a Jira avatar can redirect to a public
        // CDN, but the account's Authorization header must never follow it.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(std::time::Duration::from_secs(8)))
            .build()
            .into();
        for _ in 0..5 {
            let local_fixture = cfg!(test) && address.host_str() == Some("127.0.0.1");
            if (address.scheme() != "https" && !local_fixture)
                || !address.username().is_empty()
                || address.password().is_some()
            {
                bail!("Unsupported avatar address");
            }
            let mut request = agent.get(address.as_str()).header("Accept", "image/*");
            if address.origin() == site.origin() {
                request = request.header("Authorization", &self.auth);
            }
            let mut response = request.call()?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get("Location")
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| anyhow::anyhow!("Avatar redirect has no location"))?;
                address = address.join(location)?;
                continue;
            }
            if !response.status().is_success() {
                bail!("Avatar unavailable");
            }
            return Ok(response
                .body_mut()
                .with_config()
                .limit(limit)
                .read_to_vec()?);
        }
        bail!("Too many avatar redirects")
    }
    pub fn send(&self, method: &str, path: &str, body: Value) -> Result<Value> {
        let mut r = match method {
            "PUT" => self
                .agent
                .put(self.url(path))
                .header("Authorization", &self.auth)
                .send_json(body)?,
            "DELETE" => self
                .agent
                .delete(self.url(path))
                .header("Authorization", &self.auth)
                .call()?,
            _ => self
                .agent
                .post(self.url(path))
                .header("Authorization", &self.auth)
                .send_json(body)?,
        };
        r = checked(r)?;
        if r.status().as_u16() == 204 {
            return Ok(Value::Null);
        }
        let text = r.body_mut().read_to_string()?;
        Ok(if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text)?
        })
    }
    pub fn search(&self, tab: usize) -> Result<Vec<Issue>> {
        let t = &self.config.tabs[tab];
        let jql = if !t.jql.is_empty() {
            t.jql.clone()
        } else {
            if !t.filter_id.chars().all(|c| c.is_ascii_digit()) {
                bail!("Filter ID must be numeric");
            }
            format!("filter = {}", t.filter_id)
        };
        self.search_jql(&jql)
    }

    pub fn create_issue(&self, summary: &str, issue_type: Value) -> Result<CreatedIssue> {
        let data = self.send(
            "POST",
            "issue",
            json!({"fields":{
                "project":{"key":self.config.jira.default_project},
                "issuetype":{"id":issue_type["id"]}, "summary":summary
            }}),
        )?;
        let key = data["key"]
            .as_str()
            .filter(|key| crate::related::valid_key(key))
            .ok_or_else(|| anyhow::anyhow!("Jira did not return a valid created issue key"))?;
        // Once POST succeeds, a failed detail fetch must not offer another POST.
        Ok(match self.issue(key) {
            Ok(issue) => CreatedIssue {
                issue,
                detail_error: None,
            },
            Err(error) => CreatedIssue {
                issue: Issue {
                    key: key.into(),
                    fields: json!({"summary":summary,"issuetype":issue_type}),
                },
                detail_error: Some(error.to_string()),
            },
        })
    }

    pub fn related(&self, issue: &Issue) -> Result<Vec<Issue>> {
        self.search_jql(&crate::related::query(issue)?)
    }

    pub fn preview_jql(&self, jql: &str) -> Result<(Vec<Issue>, bool)> {
        let data = self.send(
            "POST",
            "search/jql",
            json!({
                "jql": jql, "maxResults": 50,
                "fields": ["summary", "status", "issuetype"]
            }),
        )?;
        let issues = serde_json::from_value(data["issues"].clone())?;
        let more =
            data["isLast"].as_bool() != Some(true) && data["nextPageToken"].as_str().is_some();
        Ok((issues, more))
    }

    fn search_jql(&self, jql: &str) -> Result<Vec<Issue>> {
        let mut issues = self.search_fields(
            jql,
            &[
                "summary",
                "status",
                "priority",
                "assignee",
                "reporter",
                "issuetype",
                "project",
                "created",
                "updated",
                "duedate",
                "parent",
            ],
        )?;
        // The full list request owns parent data even if Jira omits an empty value.
        for issue in &mut issues {
            if let Some(fields) = issue.fields.as_object_mut() {
                fields.entry("parent").or_insert(Value::Null);
            }
        }
        Ok(issues)
    }

    fn search_fields(&self, jql: &str, fields: &[&str]) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();
        let mut token = Value::Null;
        loop {
            let mut body = json!({"jql":jql,"maxResults":100,"fields":fields});
            if !token.is_null() {
                body["nextPageToken"] = token;
            }
            let data = self.send("POST", "search/jql", body)?;
            issues.extend(serde_json::from_value::<Vec<Issue>>(
                data["issues"].clone(),
            )?);
            token = data["nextPageToken"].clone();
            if data["isLast"].as_bool() == Some(true) || token.is_null() {
                break;
            }
        }
        Ok(issues)
    }
    pub fn issue(&self, key: &str) -> Result<Issue> {
        let mut issue: Issue = serde_json::from_value(self.get(&format!("issue/{key}"))?)?;
        let total = issue.fields["comment"]["total"].as_u64().unwrap_or(0) as usize;
        let mut comments = issue.fields["comment"]["comments"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        while comments.len() < total {
            let data = self.get(&format!(
                "issue/{key}/comment?startAt={}&maxResults=100",
                comments.len()
            ))?;
            let page = data["comments"].as_array().cloned().unwrap_or_default();
            if page.is_empty() {
                break;
            }
            comments.extend(page);
        }
        if total > 0 {
            issue.fields["comment"]["comments"] = json!(comments);
        }
        Ok(issue)
    }
}
fn checked(mut r: ureq::http::Response<ureq::Body>) -> Result<ureq::http::Response<ureq::Body>> {
    if !r.status().is_success() {
        let status = r.status();
        let message = r.body_mut().read_to_string().unwrap_or_default();
        bail!(
            "Jira returned {status}: {}",
            message.chars().take(1000).collect::<String>()
        );
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };
    fn mock(replies: Vec<(u16, Value)>) -> (String, thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.trim().to_string();
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                requests.push(json!({"request":path,"body":serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null)}));
                let body = if status == 204 {
                    String::new()
                } else {
                    body.to_string()
                };
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (url, handle)
    }
    #[test]
    fn list_dates_keep_the_calendar_day_and_raw_timestamps() {
        let issue = Issue {
            key: "T-1".into(),
            fields: json!({
                "updated":"2026-10-27T23:33:48.174-0600",
                "created":"2026-01-01T00:15:00+14:00",
                "duedate":"2024-02-29", "summary":"2026-10-27T23:33:48.174-0600"
            }),
        };
        assert_eq!(issue.list_field("updated"), "Oct 27, 2026");
        assert_eq!(issue.list_field("created"), "Jan 1, 2026");
        assert_eq!(issue.list_field("duedate"), "Feb 29, 2024");
        assert_eq!(issue.field("updated"), "2026-10-27T23:33:48.174-0600");
        assert_eq!(issue.list_field("summary"), issue.field("summary"));
    }

    #[test]
    fn unavailable_or_unrecognized_list_dates_are_preserved() {
        for value in [
            Value::Null,
            json!(""),
            json!("Not scheduled"),
            json!("2025-02-29"),
            json!("2026-13-01"),
            json!("éééééé"),
        ] {
            let issue = Issue {
                key: "T-1".into(),
                fields: json!({"updated":value}),
            };
            assert_eq!(issue.list_field("updated"), issue.field("updated"));
        }
    }

    #[test]
    fn jql_preview_is_bounded_and_reports_more_results() {
        let (url, server) = mock(vec![(
            200,
            json!({"issues":[{"key":"T-1","fields":{"summary":"One"}}],"nextPageToken":"next","isLast":false}),
        )]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let (issues, more) = client.preview_jql("parent = \"T-10\"").unwrap();
        assert_eq!(issues.len(), 1);
        assert!(more);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["body"]["jql"], "parent = \"T-10\"");
        assert_eq!(requests[0]["body"]["maxResults"], 50);
    }
    #[test]
    fn jql_preview_preserves_jira_validation_errors() {
        let (url, server) = mock(vec![(400, json!({"errorMessages":["Invalid JQL"]}))]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        assert!(
            client
                .preview_jql("invalid")
                .err()
                .unwrap()
                .to_string()
                .contains("Invalid JQL")
        );
        server.join().unwrap();
    }

    #[test]
    fn parent_details_use_issue_color_and_fall_back_to_the_epic_endpoint() {
        let (url, server) = mock(vec![
            (
                200,
                json!([{"id":"customfield_12345","name":"Issue color"}]),
            ),
            (
                200,
                json!({"isLast":true,"issues":[
                    {"key":"T-1","fields":{"issuetype":{"hierarchyLevel":1},"customfield_12345":"teal"}},
                    {"key":"T-2","fields":{"issuetype":{"name":"Epic"}}},
                    {"key":"T-3","fields":{"issuetype":{"name":"Task"},"customfield_12345":"teal"}}
                ]}),
            ),
            (200, json!({"color":{"key":"color_4"}})),
        ]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let keys = ["T-1", "T-2", "T-3", "T-4"].map(String::from);
        let details = client.parent_details(&keys).unwrap();
        assert_eq!(details.len(), 3);
        assert_eq!(details[0].color.as_deref(), Some("teal"));
        assert_eq!(details[1].color.as_deref(), Some("color_4"));
        assert!(details[2].color.is_none());
        let requests = server.join().unwrap();
        assert_eq!(requests[0]["request"], "GET /rest/api/3/field HTTP/1.1");
        assert_eq!(
            requests[1]["body"]["fields"],
            json!(["summary", "issuetype", "customfield_12345"])
        );
        assert_eq!(
            requests[2]["request"],
            "GET /rest/agile/1.0/epic/T-2 HTTP/1.1"
        );
    }

    #[test]
    fn list_refresh_clears_a_parent_omitted_from_the_response() {
        let (url, server) = mock(vec![(
            200,
            json!({"isLast":true,"issues":[
                {"key":"T-1","fields":{"summary":"No parent"}}
            ]}),
        )]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let mut store = crate::store::Store::default();
        store.merge_issue(Issue {
            key: "T-1".into(),
            fields: json!({"parent":{"key":"T-2"}}),
        });
        store.set_tab(0, client.search(0).unwrap());
        assert_eq!(store.issues["T-1"].fields.get("parent"), Some(&Value::Null));
        server.join().unwrap();
    }

    #[test]
    fn search_uses_token_pagination() {
        let (url, server) = mock(vec![
            (
                200,
                json!({"issues":[{"key":"T-1","fields":{"summary":"One"}}],"nextPageToken":"next","isLast":false}),
            ),
            (
                200,
                json!({"issues":[{"key":"T-2","fields":{"summary":"Two"}}],"isLast":true}),
            ),
        ]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let issues = client.search(0).unwrap();
        assert_eq!(issues.len(), 2);
        let requests = server.join().unwrap();
        assert_eq!(
            requests[0]["request"],
            "POST /rest/api/3/search/jql HTTP/1.1"
        );
        assert_eq!(requests[1]["body"]["nextPageToken"], "next");
    }
    #[test]
    fn update_accepts_empty_204() {
        let (url, server) = mock(vec![(204, Value::Null)]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        assert!(
            client
                .send("PUT", "issue/T-1", json!({"fields":{"summary":"Updated"}}))
                .unwrap()
                .is_null()
        );
        server.join().unwrap();
    }
    #[test]
    fn attachment_download_uses_validated_site_endpoint() {
        let (url, server) = mock(vec![(200, json!("image fixture"))]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        for id in [
            "",
            "../other",
            "https://other.example/file",
            "12?redirect=1",
        ] {
            assert!(client.attachment_preview(id).is_err());
        }
        assert!(!client.attachment_preview("123").unwrap().is_empty());
        let requests = server.join().unwrap();
        assert_eq!(
            requests[0]["request"],
            "GET /rest/api/3/attachment/content/123 HTTP/1.1"
        );
    }

    #[test]
    fn create_returns_loaded_details_and_sends_the_selected_type() {
        let (url, server) = mock(vec![
            (201, json!({"key":"T-3"})),
            (
                200,
                json!({"key":"T-3","fields":{"summary":"New task","status":{"name":"Open"}}}),
            ),
        ]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let created = client
            .create_issue("New task", json!({"id":"10001","name":"Task"}))
            .unwrap();
        assert_eq!(created.issue.key, "T-3");
        assert_eq!(created.issue.field("status"), "Open");
        assert!(created.detail_error.is_none());
        let requests = server.join().unwrap();
        assert_eq!(requests[0]["request"], "POST /rest/api/3/issue HTTP/1.1");
        assert_eq!(
            requests[0]["body"]["fields"],
            json!({"project":{"key":"T"},"issuetype":{"id":"10001"},"summary":"New task"})
        );
        assert_eq!(requests[1]["request"], "GET /rest/api/3/issue/T-3 HTTP/1.1");
    }

    #[test]
    fn create_validation_error_does_not_fetch_details() {
        let (url, server) = mock(vec![(
            400,
            json!({"errors":{"customfield_10000":"Required field"}}),
        )]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let error = client
            .create_issue("Keep this summary", json!({"id":"10001"}))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("Required field"));
        assert_eq!(server.join().unwrap().len(), 1);
    }

    #[test]
    fn detail_failure_after_create_preserves_success_and_never_reposts() {
        let (url, server) = mock(vec![
            (201, json!({"key":"T-3"})),
            (503, json!({"errorMessages":["Unavailable"]})),
        ]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        let created = client
            .create_issue("Keep this summary", json!({"id":"10001","name":"Task"}))
            .unwrap();
        assert_eq!(created.issue.key, "T-3");
        assert_eq!(created.issue.field("summary"), "Keep this summary");
        assert_eq!(created.issue.field("issuetype"), "Task");
        assert!(created.detail_error.unwrap().contains("503"));
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1]["request"].as_str().unwrap().starts_with("GET "));
    }

    #[test]
    fn related_search_paginates_and_requests_parent_and_row_metadata() {
        let (url, server) = mock(vec![
            (
                200,
                json!({"issues":[{"key":"P-2","fields":{"parent":{"key":"P-1"},"summary":"Child"}}],"nextPageToken":"more","isLast":false}),
            ),
            (
                200,
                json!({"issues":[{"key":"P-9","fields":{"summary":"Parent"}}],"isLast":true}),
            ),
        ]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "P".into()),
            "fixture-token".into(),
        );
        let issues = client
            .related(&Issue {
                key: "P-1".into(),
                fields: json!({"parent":{"key":"P-9"}}),
            })
            .unwrap();
        assert_eq!(issues.len(), 2);
        let requests = server.join().unwrap();
        assert_eq!(
            requests[0]["body"]["jql"],
            "parent = \"P-1\" OR key IN (\"P-9\") ORDER BY key ASC"
        );
        let fields = requests[0]["body"]["fields"].as_array().unwrap();
        for field in [
            "parent",
            "summary",
            "status",
            "priority",
            "assignee",
            "issuetype",
        ] {
            assert!(fields.iter().any(|value| value == field));
        }
        assert_eq!(requests[1]["body"]["nextPageToken"], "more");
    }
    #[test]
    fn failed_auth_is_an_error() {
        let (url, server) = mock(vec![(401, json!({"errorMessages":["Unauthorized"]}))]);
        let client = Client::new(
            Config::initial(url, "test@example.com".into(), "T".into()),
            "test-token".into(),
        );
        assert!(
            client
                .get("myself")
                .unwrap_err()
                .to_string()
                .contains("401")
        );
        server.join().unwrap();
    }

    #[test]
    fn avatar_redirects_never_forward_jira_credentials_to_another_origin() {
        fn endpoint(
            location: Option<String>,
            body: &'static [u8],
        ) -> (String, thread::JoinHandle<bool>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = format!("http://{}", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut authorization = false;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    authorization |= line.to_ascii_lowercase().starts_with("authorization:");
                }
                if let Some(location) = location {
                    write!(socket, "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                    socket.write_all(body).unwrap();
                }
                authorization
            });
            (address, server)
        }
        let (cdn, public) = endpoint(None, b"image-bytes");
        let (site, jira) = endpoint(Some(format!("{cdn}/photo.png")), b"");
        let client = Client::new(
            Config::initial(site.clone(), "test@example.com".into(), "T".into()),
            "fixture-token".into(),
        );
        assert_eq!(
            client.avatar_bytes(&format!("{site}/avatar")).unwrap(),
            b"image-bytes"
        );
        assert!(jira.join().unwrap());
        assert!(!public.join().unwrap());
    }

    #[test]
    fn avatar_downloader_rejects_insecure_and_embedded_credential_urls() {
        let client = Client::new(
            Config::initial(
                "https://jira.example".into(),
                "test@example.com".into(),
                "T".into(),
            ),
            "fixture-token".into(),
        );
        for address in [
            "http://avatar.example/photo.png",
            "https://name:password@avatar.example/photo.png",
            "file:///avatar.png",
        ] {
            assert_eq!(
                client.avatar_bytes(address).unwrap_err().to_string(),
                "Unsupported avatar address"
            );
        }
    }
}
