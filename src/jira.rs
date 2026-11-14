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

    pub fn avatar_bytes(&self, address: &str) -> Result<Vec<u8>> {
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
                .limit(1024 * 1024)
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

    pub fn related(&self, issue: &Issue) -> Result<Vec<Issue>> {
        self.search_jql(&crate::related::query(issue)?)
    }

    fn search_jql(&self, jql: &str) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();
        let mut token = Value::Null;
        loop {
            let mut body = json!({"jql":jql,"maxResults":100,"fields":["summary","status","priority","assignee","reporter","issuetype","project","created","updated","duedate","parent"]});
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
