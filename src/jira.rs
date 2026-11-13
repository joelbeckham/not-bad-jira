use crate::config::Config;
use anyhow::{Result, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    #[serde(default)]
    pub fields: Value,
}
impl Issue {
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
        }
    }
    fn url(&self, path: &str) -> String {
        format!("{}/rest/api/3/{}", self.config.jira.base_url, path)
    }
    pub fn get(&self, path: &str) -> Result<Value> {
        Ok(checked(
            self.agent
                .get(self.url(path))
                .header("Authorization", &self.auth)
                .call()?,
        )?
        .body_mut()
        .read_json()?)
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
        let mut issues = Vec::new();
        let mut token = Value::Null;
        loop {
            let mut body = json!({"jql":jql,"maxResults":100,"fields":["summary","status","priority","assignee","reporter","issuetype","project","created","updated","duedate"]});
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
}
