//! Lossless protected-node Markdown editing. Unsupported nodes remain opaque.
use anyhow::{Result, bail};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde_json::{Value, json};
#[derive(Default)]
pub struct Document {
    pub markdown: String,
    protected: Vec<Value>,
    task_ids: Vec<String>,
    list_ids: Vec<String>,
}
impl Document {
    pub fn display(value: &Value) -> String {
        fn text(v: &Value) -> String {
            let content = v["content"]
                .as_array()
                .map(|a| a.iter().map(text).collect::<String>())
                .unwrap_or_default();
            match v["type"].as_str().unwrap_or("") {
                "text" => {
                    let mut s = v["text"]
                        .as_str()
                        .unwrap_or("")
                        .replace("\r\n", "\n")
                        .replace('\r', "\n")
                        .replace('\n', "  \n");
                    for m in v["marks"].as_array().into_iter().flatten() {
                        s = match m["type"].as_str().unwrap_or("") {
                            "strong" => format!("**{s}**"),
                            "em" => format!("*{s}*"),
                            "strike" => format!("~~{s}~~"),
                            "code" => format!("`{s}`"),
                            "link" => {
                                format!("[{s}]({})", m["attrs"]["href"].as_str().unwrap_or(""))
                            }
                            _ => s,
                        };
                    }
                    s
                }
                "paragraph" => format!("{content}\n\n"),
                "heading" => format!(
                    "{} {content}\n\n",
                    "#".repeat(v["attrs"]["level"].as_u64().unwrap_or(1) as usize)
                ),
                "codeBlock" => format!(
                    "```{}\n{}\n```\n\n",
                    v["attrs"]["language"].as_str().unwrap_or(""),
                    v["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|node| node["text"].as_str().unwrap_or(""))
                        .collect::<String>()
                ),
                "bulletList" | "orderedList" => {
                    v["content"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .enumerate()
                                .map(|(i, c)| {
                                    format!(
                                        "{} {}\n",
                                        if v["type"] == "bulletList" {
                                            "-".into()
                                        } else {
                                            format!("{}.", i + 1)
                                        },
                                        text(c).trim().replace('\n', "\n  ")
                                    )
                                })
                                .collect::<String>()
                        })
                        .unwrap_or_default()
                        + "\n"
                }
                "blockquote" | "panel" => format!("> {}\n\n", content.trim().replace('\n', "\n> ")),
                "mention" => format!("**{}**", v["attrs"]["text"].as_str().unwrap_or("@mention")),
                "emoji" => v["attrs"]["text"]
                    .as_str()
                    .or_else(|| v["attrs"]["shortName"].as_str())
                    .unwrap_or(":emoji:")
                    .into(),
                "inlineCard" | "blockCard" => {
                    let url = v["attrs"]["url"].as_str().unwrap_or("");
                    format!("[{url}]({url})")
                }
                "status" => format!("`{}`", v["attrs"]["text"].as_str().unwrap_or("Status")),
                "media" => "[Attachment]".into(),
                "table" => {
                    let mut out = String::new();
                    for (i, row) in v["content"].as_array().into_iter().flatten().enumerate() {
                        let cells = row["content"].as_array();
                        out.push('|');
                        for cell in cells.into_iter().flatten() {
                            out.push_str(&format!(" {} |", text(cell).trim().replace('|', "\\|")));
                        }
                        out.push('\n');
                        if i == 0 {
                            out.push_str(&format!(
                                "|{}\n",
                                " --- |".repeat(cells.map(Vec::len).unwrap_or(0))
                            ));
                        }
                    }
                    out.push('\n');
                    out
                }
                "hardBreak" => "  \n".into(),
                "rule" => "\n---\n".into(),
                _ => content,
            }
        }
        text(value)
    }
    pub fn from_adf(value: &Value) -> Self {
        let mut doc = Self::default();
        if value.is_null() {
            return doc;
        }
        doc.markdown = doc.render(value);
        doc
    }
    fn protect(&mut self, v: &Value) -> String {
        let i = self.protected.len();
        self.protected.push(v.clone());
        if matches!(
            v["type"].as_str(),
            Some("mediaSingle" | "mediaGroup" | "media")
        ) {
            format!("![Jira media](jira-media:{i})\n\n")
        } else {
            format!("[ADF:{i}]")
        }
    }
    fn render(&mut self, v: &Value) -> String {
        let kind = v["type"].as_str().unwrap_or("");
        if let Some(attrs) = v["attrs"].as_object() {
            let allowed: &[&str] = match kind {
                "heading" => &["level"],
                "codeBlock" => &["language"],
                "orderedList" => &["order"],
                "taskList" => &["localId"],
                "taskItem" => &["localId", "state"],
                _ => &[],
            };
            if attrs.keys().any(|k| !allowed.contains(&k.as_str())) {
                return self.protect(v);
            }
        }
        if v.get("attrs").is_some()
            && !matches!(
                kind,
                "heading" | "codeBlock" | "orderedList" | "taskList" | "taskItem"
            )
        {
            return self.protect(v);
        }
        if kind == "text" {
            let raw = v["text"].as_str().unwrap_or("");
            if raw.contains("[ADF:") {
                return self.protect(v);
            }
            // Older saves embedded line breaks in text nodes. Recover those as
            // editable lines, retaining marks and protected unsupported content.
            if raw.contains(['\n', '\r']) {
                let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
                return normalized
                    .split('\n')
                    .map(|line| {
                        let mut node = v.clone();
                        node["text"] = json!(line);
                        self.render(&node)
                    })
                    .collect::<Vec<_>>()
                    .join("  \n");
            }
            let all_marks = v["marks"].as_array();
            if all_marks.is_some_and(|a| {
                a.iter().any(|m| m["type"] == "code") && (a.len() > 1 || raw.trim().is_empty())
            }) {
                return self.protect(v);
            }
            let mut text = raw
                .char_indices()
                .map(|(i, c)| {
                    // Keep ordinary prose and URLs readable. Retain escapes for
                    // Markdown syntax, including possible block markers at node boundaries.
                    let before = raw[..i].chars().next_back();
                    let after = raw[i + c.len_utf8()..].chars().next();
                    let literal = match c {
                        ':' | '/' | ';' | ',' | '@' | '$' | '%' | '^' | '?' | '{' | '}' => true,
                        '.' => {
                            after.is_some_and(|c| !c.is_whitespace())
                                || before.is_some_and(|c| !c.is_ascii_digit())
                        }
                        '-' => {
                            before.is_some_and(char::is_alphanumeric)
                                && after.is_some_and(char::is_alphanumeric)
                        }
                        _ => !c.is_ascii_punctuation(),
                    };
                    if !literal {
                        format!("\\{c}")
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>();
            for mark in v["marks"].as_array().into_iter().flatten() {
                text = match mark["type"].as_str().unwrap_or("") {
                    "strong" => format!("**{text}**"),
                    "em" => format!("*{text}*"),
                    "strike" => format!("~~{text}~~"),
                    "code" => {
                        let fence = "`"
                            .repeat(raw.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
                        format!("{fence} {raw} {fence}")
                    }
                    "link" => format!("[{text}]({})", mark["attrs"]["href"].as_str().unwrap_or("")),
                    _ => return self.protect(v),
                };
            }
            return text;
        }
        if kind == "table" {
            let Some(rows) = v["content"].as_array() else {
                return self.protect(v);
            };
            let count = rows
                .first()
                .and_then(|r| r["content"].as_array())
                .map(Vec::len)
                .unwrap_or(0);
            if count == 0
                || rows.iter().enumerate().any(|(i, r)| {
                    r["content"].as_array().is_none_or(|cells| {
                        cells.len() != count
                            || cells.iter().any(|c| {
                                c.get("attrs").is_some()
                                    || (i == 0 && c["type"] != "tableHeader")
                                    || c["content"]
                                        .as_array()
                                        .is_none_or(|a| a.len() != 1 || a[0]["type"] != "paragraph")
                            })
                    })
                })
            {
                return self.protect(v);
            }
            let mut out = String::new();
            for (i, row) in rows.iter().enumerate() {
                out.push('|');
                for cell in row["content"].as_array().unwrap() {
                    let s = self.render(&cell["content"][0]);
                    out.push_str(&format!(" {} |", s.trim().replace('|', "\\|")));
                }
                out.push('\n');
                if i == 0 {
                    out.push_str(&format!("|{}\n", " --- |".repeat(count)));
                }
            }
            out.push('\n');
            return out;
        }
        if !matches!(
            kind,
            "doc"
                | "paragraph"
                | "heading"
                | "bulletList"
                | "orderedList"
                | "listItem"
                | "blockquote"
                | "codeBlock"
                | "rule"
                | "hardBreak"
                | "taskList"
                | "taskItem"
        ) {
            return self.protect(v);
        }
        if kind == "codeBlock" {
            let raw = v["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| n["text"].as_str().unwrap_or(""))
                .collect::<String>();
            let fence =
                "`".repeat((raw.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1).max(3));
            return format!(
                "{fence}{}\n{raw}\n{fence}\n\n",
                v["attrs"]["language"].as_str().unwrap_or("")
            );
        }
        if matches!(kind, "bulletList" | "orderedList" | "taskList") {
            if kind == "taskList" {
                self.list_ids
                    .push(v["attrs"]["localId"].as_str().unwrap_or("").into());
            }
            let mut out = String::new();
            for (i, item) in v["content"].as_array().into_iter().flatten().enumerate() {
                let prefix = if kind == "orderedList" {
                    format!("{}.", i as u64 + v["attrs"]["order"].as_u64().unwrap_or(1))
                } else {
                    "-".into()
                };
                let body = self.render(item);
                let body = body.trim_end();
                out.push_str(&format!(
                    "{prefix} {}\n",
                    body.replace('\n', &format!("\n{}", " ".repeat(prefix.len() + 1)))
                ));
            }
            out.push('\n');
            return out;
        }
        if kind == "taskItem" {
            self.task_ids
                .push(v["attrs"]["localId"].as_str().unwrap_or("").into());
            let inner = v["content"]
                .as_array()
                .map(|a| a.iter().map(|c| self.render(c)).collect::<String>())
                .unwrap_or_default();
            return format!(
                "[{}] {inner}",
                if v["attrs"]["state"] == "DONE" {
                    "x"
                } else {
                    " "
                }
            );
        }
        let inner = v["content"]
            .as_array()
            .map(|a| a.iter().map(|c| self.render(c)).collect::<String>())
            .unwrap_or_default();
        match kind {
            "paragraph" => format!("{inner}\n\n"),
            "heading" => format!(
                "{} {inner}\n\n",
                "#".repeat(v["attrs"]["level"].as_u64().unwrap_or(1) as usize)
            ),
            "blockquote" => format!("> {}\n\n", inner.trim().replace('\n', "\n> ")),
            "rule" => "\n---\n\n".into(),
            "hardBreak" => "  \n".into(),
            _ => inner,
        }
    }
    pub fn to_adf(&self, markdown: &str) -> Result<Value> {
        let mut normalized = markdown.to_string();
        for (i, node) in self.protected.iter().enumerate() {
            if matches!(
                node["type"].as_str(),
                Some("mediaSingle" | "mediaGroup" | "media")
            ) {
                normalized = normalized.replace(
                    &format!("![Jira media](jira-media:{i})"),
                    &format!("[ADF:{i}]"),
                );
            }
        }
        let markdown = normalized.as_str();
        for i in 0..self.protected.len() {
            if markdown.matches(&format!("[ADF:{i}]")).count() != 1 {
                bail!(
                    "Protected content [ADF:{i}] was changed or removed. Restore it before saving."
                );
            }
        }
        let mut stack = vec![json!({"type":"doc","version":1,"content":[]})];
        let mut marks = Vec::<Value>::new();
        let mut table_head = false;
        let mut task_counter = 0usize;
        let mut list_counter = 0usize;
        for event in Parser::new_ext(
            markdown,
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
        ) {
            match event {
                Event::Start(tag)=>match tag {
                    Tag::Emphasis=>marks.push(json!({"type":"em"})),Tag::Strong=>marks.push(json!({"type":"strong"})),Tag::Strikethrough=>marks.push(json!({"type":"strike"})),Tag::Link{dest_url,..}=>marks.push(json!({"type":"link","attrs":{"href":dest_url.to_string()}})),
                    Tag::Paragraph=>stack.push(json!({"type":"paragraph","content":[]})),
                    Tag::Heading{level,..}=>stack.push(json!({"type":"heading","attrs":{"level":level as u8},"content":[]})),
                    Tag::BlockQuote(_)=>stack.push(json!({"type":"blockquote","content":[]})),
                    Tag::List(start)=>stack.push(if let Some(start)=start{json!({"type":"orderedList","attrs":{"order":start},"content":[]})}else{json!({"type":"bulletList","content":[]})}),
                    Tag::Item=>stack.push(json!({"type":"listItem","content":[]})),
                    Tag::CodeBlock(kind)=>stack.push(json!({"type":"codeBlock","attrs":{"language":match kind{CodeBlockKind::Fenced(s)=>s.to_string(),_=>String::new()}},"content":[]})),
                    Tag::Table(_)=>stack.push(json!({"type":"table","content":[]})),
                    Tag::TableHead=>{table_head=true;stack.push(json!({"type":"tableRow","content":[]}));},
                    Tag::TableRow=>stack.push(json!({"type":"tableRow","content":[]})),
                    Tag::TableCell=>stack.push(json!({"type":if table_head{"tableHeader"}else{"tableCell"},"content":[]})),
                    _=>bail!("This Markdown structure is not supported yet; your text has been kept."),
                },
                Event::End(tag)=>match tag {TagEnd::Emphasis|TagEnd::Strong|TagEnd::Strikethrough|TagEnd::Link=>{marks.pop();},_=>{if tag==TagEnd::TableHead{table_head=false;}
if stack.len()>1 {let mut node=stack.pop().unwrap();finish_node(&mut node);push(&mut stack,node);}}},
                Event::Text(text)=>{
                    // Protected references may be split by the Markdown parser's bracket handling.
                    let mut node=json!({"type":"text","text":text.to_string()});if !marks.is_empty(){node["marks"]=json!(marks);}push(&mut stack,node);
                },
                Event::Code(text)=>{let mut active=marks.clone();active.push(json!({"type":"code"}));push(&mut stack,json!({"type":"text","text":text.to_string(),"marks":active}));},
                Event::SoftBreak=>push(&mut stack,json!({"type":"hardBreak"})),
                Event::HardBreak=>push(&mut stack,json!({"type":"hardBreak"})),Event::Rule=>push(&mut stack,json!({"type":"rule"})),
                Event::TaskListMarker(done)=>{task_counter+=1;let item=stack.iter_mut().rev().find(|n|n["type"]=="listItem").ok_or_else(||anyhow::anyhow!("Checklist item is outside a list"))?;item["type"]=json!("taskItem");item["attrs"]=json!({"localId":self.task_ids.get(task_counter-1).cloned().unwrap_or_else(||format!("task-{}-{task_counter}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos())),"state":if done{"DONE"}else{"TODO"}});let list=stack.iter_mut().rev().find(|n|n["type"]=="bulletList"||n["type"]=="taskList").ok_or_else(||anyhow::anyhow!("Checklist requires a bullet list"))?;if list["type"]=="bulletList"{list_counter+=1;list["type"]=json!("taskList");list["attrs"]=json!({"localId":self.list_ids.get(list_counter-1).cloned().unwrap_or_else(||format!("list-{}-{list_counter}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()))});}},
                _=>bail!("This Markdown feature cannot be saved safely yet."),
            }
        }
        let mut root = stack.remove(0);
        restore(&mut root, &self.protected)?;
        Ok(root)
    }
}
fn finish_node(node: &mut Value) {
    if node["type"] == "codeBlock"
        && let Some(a) = node["content"].as_array_mut()
        && let Some(last) = a.last_mut()
        && let Some(s) = last["text"].as_str()
    {
        last["text"] = json!(s.strip_suffix('\n').unwrap_or(s));
    }
    if matches!(
        node["type"].as_str(),
        Some("tableCell" | "tableHeader" | "listItem")
    ) {
        let children = node["content"].as_array_mut().unwrap();
        let mut result = Vec::new();
        let mut inline = Vec::new();
        for c in children.drain(..) {
            if matches!(
                c["type"].as_str(),
                Some("text" | "hardBreak" | "mention" | "emoji" | "inlineCard")
            ) {
                inline.push(c);
            } else {
                if !inline.is_empty() {
                    result.push(json!({"type":"paragraph","content":std::mem::take(&mut inline)}));
                }
                result.push(c);
            }
        }
        if !inline.is_empty() {
            result.push(json!({"type":"paragraph","content":inline}));
        }
        if result.is_empty() {
            result.push(json!({"type":"paragraph","content":[]}));
        }
        *children = result;
    }
    if node["type"] == "taskItem" {
        let a = node["content"].as_array_mut().unwrap();
        let old = std::mem::take(a);
        for c in old {
            if c["type"] == "paragraph" {
                a.extend(c["content"].as_array().into_iter().flatten().cloned());
            } else {
                a.push(c);
            }
        }
    }
}
fn push(stack: &mut [Value], node: Value) {
    stack.last_mut().unwrap()["content"]
        .as_array_mut()
        .unwrap()
        .push(node);
}
fn restore(v: &mut Value, protected: &[Value]) -> Result<()> {
    if let Some(children) = v["content"].as_array_mut() {
        // Merge adjacent unmarked text runs before expanding token references.
        let mut merged: Vec<Value> = Vec::new();
        for child in children.drain(..) {
            if child["type"] == "text"
                && child.get("marks").is_none()
                && merged
                    .last()
                    .is_some_and(|p| p["type"] == "text" && p.get("marks").is_none())
            {
                let prev = merged.last_mut().unwrap();
                prev["text"] = json!(format!(
                    "{}{}",
                    prev["text"].as_str().unwrap(),
                    child["text"].as_str().unwrap()
                ));
            } else {
                merged.push(child);
            }
        }
        for mut child in merged {
            if child["type"] == "text" {
                let text = child["text"].as_str().unwrap_or("").to_string();
                let mut rest = text.as_str();
                while let Some(pos) = rest.find("[ADF:") {
                    if pos > 0 {
                        children.push(json!({"type":"text","text":&rest[..pos]}));
                    }
                    let suffix = &rest[pos + 5..];
                    let end = suffix
                        .find(']')
                        .ok_or_else(|| anyhow::anyhow!("Invalid protected token"))?;
                    let i: usize = suffix[..end].parse()?;
                    children.push(
                        protected
                            .get(i)
                            .ok_or_else(|| anyhow::anyhow!("Unknown protected token"))?
                            .clone(),
                    );
                    rest = &suffix[end + 1..];
                }
                if rest != text {
                    if !rest.is_empty() {
                        children.push(json!({"type":"text","text":rest}));
                    }
                } else {
                    children.push(child);
                }
            } else {
                restore(&mut child, protected)?;
                children.push(child);
            }
        }
    }
    // A paragraph wrapping a protected block is a Markdown artifact.
    if v["type"] == "paragraph"
        && let Some(a) = v["content"].as_array()
        && a.len() == 1
        && !matches!(
            a[0]["type"].as_str(),
            Some("text" | "mention" | "emoji" | "hardBreak" | "inlineCard" | "date" | "status")
        )
    {
        *v = a[0].clone();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jira_media_survives_text_edits_and_rejects_missing_tokens() {
        for node in [
            json!({"type":"mediaSingle","attrs":{"layout":"center","width":80},"content":[{"type":"media","attrs":{"id":"uuid-image","type":"file","collection":"jira","width":800,"height":600}}]}),
            json!({"type":"mediaGroup","content":[{"type":"media","attrs":{"id":"uuid-pdf","type":"file","collection":"jira"}}]}),
        ] {
            let original = json!({"type":"doc","version":1,"content":[node]});
            let doc = Document::from_adf(&original);
            assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
            let edited = doc
                .to_adf(&format!("Changed text\n\n{}", doc.markdown))
                .unwrap();
            assert_eq!(edited["content"][1], original["content"][0]);
            assert!(doc.to_adf("").is_err());
            assert!(
                doc.to_adf(&format!("{}\n\n{}", doc.markdown, doc.markdown))
                    .is_err()
            );
        }
    }
    #[test]
    fn entered_lines_survive_save_and_reopen() {
        let input = "34.216.59.33\n54.69.240.210\n35.90.42.16\n52.27.217.135\n34.215.163.132";
        let saved = Document::default().to_adf(input).unwrap();
        assert_eq!(
            saved["content"][0]["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|n| n["type"] == "hardBreak")
                .count(),
            4
        );
        let reopened = Document::from_adf(&saved);
        assert!(!reopened.markdown.contains("\\."));
        assert!(!reopened.markdown.contains("[ADF:"));
        assert_eq!(reopened.to_adf(&reopened.markdown).unwrap(), saved);
        let legacy = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":input}]}]});
        let recovered = Document::from_adf(&legacy);
        assert!(!recovered.markdown.contains("[ADF:"));
        assert_eq!(recovered.to_adf(&recovered.markdown).unwrap(), saved);
        assert_eq!(
            Document::display(&legacy),
            format!("{}\n\n", input.replace('\n', "  \n"))
        );
    }
    #[test]
    fn standard_block_roundtrips() {
        for node in [
            json!({"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"First item"}]}]}]}),
            json!({"type":"codeBlock","attrs":{"language":"rust"},"content":[{"type":"text","text":"let x = 1;"}]}),
            json!({"type":"taskList","attrs":{"localId":"list-id"},"content":[{"type":"taskItem","attrs":{"localId":"item-id","state":"DONE"},"content":[{"type":"text","text":"Complete"}]}]}),
        ] {
            let original = json!({"type":"doc","version":1,"content":[node]});
            let doc = Document::from_adf(&original);
            assert_eq!(
                doc.to_adf(&doc.markdown).unwrap(),
                original,
                "{}",
                doc.markdown
            );
        }
    }
    #[test]
    fn table_roundtrip() {
        let original = Document::default()
            .to_adf("| Name | State |\n| --- | --- |\n| One | Done |\n")
            .unwrap();
        let doc = Document::from_adf(&original);
        assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
    }
    #[test]
    fn markdown_structures_are_valid() {
        let doc = Document::default();
        let a=doc.to_adf("- first\n- second\n\n| Name | State |\n| --- | --- |\n| One | Done |\n\n- [x] finished\n").unwrap();
        assert_eq!(a["content"][0]["type"], "bulletList");
        assert_eq!(
            a["content"][0]["content"][0]["content"][0]["type"],
            "paragraph"
        );
        assert_eq!(a["content"][1]["type"], "table");
        assert_eq!(
            a["content"][1]["content"][0]["content"][0]["type"],
            "tableHeader"
        );
        assert_eq!(a["content"][2]["content"][0]["attrs"]["state"], "DONE");
    }
    #[test]
    fn literal_markdown_is_not_reinterpreted() {
        for text in [
            "1. literal list",
            "# literal heading",
            "> literal quote",
            "a * b [x](url)",
            "---",
        ] {
            let original = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]});
            let doc = Document::from_adf(&original);
            assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
        }
    }
    #[test]
    fn editor_keeps_urls_and_prose_readable() {
        let text = "Invited James to app.bombbomb.com repo. GitHub username: at-1337-r";
        let url = "https://github.com/bombbomb/app.bombbomb.com/settings/access";
        let original = json!({"type":"doc","version":1,"content":[
            {"type":"paragraph","content":[{"type":"text","text":text}]},
            {"type":"paragraph","content":[{"type":"text","text":url}]}
        ]});
        let doc = Document::from_adf(&original);
        assert_eq!(doc.markdown, format!("{text}\n\n{url}\n\n"));
        assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
        let mut linked = original.clone();
        linked["content"][1]["content"][0]["marks"] = json!([{"type":"link","attrs":{"href":url}}]);
        let doc = Document::from_adf(&linked);
        assert_eq!(doc.markdown, format!("{text}\n\n[{url}]({url})\n\n"));
        assert_eq!(doc.to_adf(&doc.markdown).unwrap(), linked);
    }
    #[test]
    fn backticks_in_inline_code_survive() {
        let original = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"a ` b","marks":[{"type":"code"}]}]}]});
        let doc = Document::from_adf(&original);
        assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
    }
    #[test]
    fn duplicate_protected_token_is_rejected() {
        let doc = Document::from_adf(
            &json!({"type":"doc","version":1,"content":[{"type":"mediaSingle","content":[{"type":"media","attrs":{"id":"attachment"}}]}]}),
        );
        assert!(
            doc.to_adf(&format!("{}{}", doc.markdown, doc.markdown))
                .is_err()
        );
    }
    #[test]
    fn unknown_content_survives() {
        let original = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"abc","text":"Jane"}}]},{"type":"panel","attrs":{"panelType":"info"},"content":[{"type":"paragraph","content":[{"type":"text","text":"Keep this"}]}]}]});
        let doc = Document::from_adf(&original);
        assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
        assert!(doc.to_adf("").is_err());
    }
    #[test]
    fn formatting_survives() {
        let original = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"hello","marks":[{"type":"strong"}]}]}]});
        let doc = Document::from_adf(&original);
        assert_eq!(doc.to_adf(&doc.markdown).unwrap(), original);
    }
}
