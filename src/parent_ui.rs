use super::*;
use crate::jira::ParentDetails;
use std::{collections::HashSet, time::Instant};

pub(super) const WIDTH: f32 = 220.;
const COLOR_TTL: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct Parents {
    details: HashMap<String, (Instant, Option<ParentDetails>)>,
    pending: bool,
    next_scan: Option<Instant>,
}

fn parent_key(issue: &Issue) -> Option<&str> {
    issue.fields["parent"]["key"]
        .as_str()
        .filter(|key| *key != issue.key && crate::related::valid_key(key))
}

impl AppView {
    fn parent_field<'a>(&'a self, issue: &'a Issue, key: &str, field: &str) -> &'a Value {
        self.store
            .issues
            .get(key)
            .and_then(|parent| parent.fields.get(field))
            .filter(|value| !value.is_null())
            .or_else(|| {
                self.parents
                    .details
                    .get(key)?
                    .1
                    .as_ref()?
                    .issue
                    .fields
                    .get(field)
                    .filter(|value| !value.is_null())
            })
            .unwrap_or(&issue.fields["parent"]["fields"][field])
    }

    fn parent_label<'a>(&'a self, issue: &'a Issue, key: &'a str) -> &'a str {
        self.parent_field(issue, key, "summary")
            .as_str()
            .filter(|summary| !summary.trim().is_empty())
            .unwrap_or(key)
    }

    pub(super) fn parent_sort(&self, issue: &Issue) -> String {
        parent_key(issue)
            .map(|key| self.parent_label(issue, key).to_lowercase())
            .unwrap_or_default()
    }

    pub(super) fn request_parents(&mut self) {
        let Some(client) = self.client.as_ref() else {
            return;
        };
        let now = Instant::now();
        if self.parents.pending
            || !self.stack.is_empty()
            || self.tab_editor.is_some()
            || self.parents.next_scan.is_some_and(|next| now < next)
            || !client
                .config
                .tabs
                .get(self.tab)
                .is_some_and(|tab| tab.columns.iter().any(|field| field == "parent"))
        {
            return;
        }
        self.parents.next_scan = Some(now + Duration::from_secs(1));
        let mut seen = HashSet::new();
        let keys: Vec<_> = self
            .store
            .tabs
            .get(self.tab)
            .into_iter()
            .flatten()
            .filter_map(|key| self.store.issues.get(key))
            .filter_map(|issue| {
                let key = parent_key(issue)?;
                let kind = self.parent_field(issue, key, "issuetype");
                (crate::tabs::is_epic_type(kind) || kind.is_null()).then_some(key)
            })
            .filter(|&key| {
                self.parents
                    .details
                    .get(key)
                    .is_none_or(|(when, _)| now.duration_since(*when) >= COLOR_TTL)
            })
            .filter(|&key| seen.insert(key))
            .take(12)
            .map(str::to_owned)
            .collect();
        if keys.is_empty() {
            return;
        }

        self.parents.pending = true;
        let client = client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.parent_details(&keys).ok();
            let _ = tx.send(Message::Parents(keys, result));
        });
    }

    pub(super) fn apply_parents(&mut self, keys: Vec<String>, result: Option<Vec<ParentDetails>>) {
        self.parents.pending = false;
        let succeeded = result.is_some();
        let mut by_key: HashMap<_, _> = result
            .into_iter()
            .flatten()
            .map(|parent| (parent.issue.key.clone(), parent))
            .collect();
        let now = Instant::now();
        for key in keys {
            let details = by_key.remove(&key);
            let entry = self.parents.details.entry(key).or_insert((now, None));
            entry.0 = now;
            if succeeded {
                entry.1 = details;
            }
        }
    }

    pub(super) fn parent_cell(
        &self,
        issue: &Issue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let cell = div()
            .w(px(WIDTH))
            .pr_3()
            .flex_shrink_0()
            .flex()
            .items_center();
        let Some(key) = parent_key(issue) else {
            return cell;
        };
        let summary = self.parent_label(issue, key).to_owned();
        let kind = self.parent_field(issue, key, "issuetype");
        let epic = crate::tabs::is_epic_type(kind);
        let details = self
            .parents
            .details
            .get(key)
            .and_then(|entry| entry.1.as_ref());
        let label_color = details
            .and_then(|details| details.color.as_deref())
            .and_then(label_color);
        let tooltip = if summary == key {
            key.to_owned()
        } else {
            format!("{key} · {summary}")
        };
        let parent = Issue {
            key: key.to_owned(),
            fields: json!({"summary":self.parent_field(issue, key, "summary"),"issuetype":kind}),
        };
        let child_key = issue.key.clone();
        cell.child(
            div()
                .id("parent-link")
                .role(Role::Link)
                .aria_label(tooltip.clone())
                .min_w_0()
                .max_w_full()
                .h(px(20.))
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .hover(|label| label.underline())
                .when(epic, |label| {
                    label
                        .rounded(px(4.))
                        .px_2()
                        .bg(label_color.unwrap_or_else(|| rgb(0x313a48).into()))
                        .text_color(rgb(if label_color.is_some() {
                            0x111822
                        } else {
                            0xc0cad8
                        }))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                })
                .when(!epic, |label| {
                    label.text_color(rgb(0xa4afc0)).child(self.type_icon(
                        kind,
                        "parent-type",
                        window,
                        cx,
                    ))
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .when(summary == key, |label| label.font_family("JetBrains Mono"))
                        .child(summary),
                )
                .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) = this.keys(cx).iter().position(|key| key == &child_key) {
                        this.selected = index;
                        this.list_wheel = Default::default();
                    }
                    this.store
                        .issues
                        .entry(parent.key.clone())
                        .or_insert_with(|| parent.clone());
                    this.open(parent.key.clone());
                    this.focus.focus(window, cx);
                    cx.notify();
                })),
        )
    }

    pub(super) fn demo_parents(&mut self) {
        self.config.as_mut().unwrap().tabs[0].columns = ["key", "priority", "summary", "parent"]
            .map(String::from)
            .to_vec();
        let epic = self.store.issues.get_mut("NBJ-104").unwrap();
        epic.fields["summary"] = json!("Native workspace and keyboard navigation");
        self.parents.details.insert(
            epic.key.clone(),
            (
                Instant::now(),
                Some(ParentDetails {
                    issue: epic.clone(),
                    color: Some("purple".into()),
                }),
            ),
        );
        let infra = Issue {
            key: "NBJ-108".into(),
            fields: json!({
                "summary":"Infrastructure reliability", "issuetype":{"name":"Epic","hierarchyLevel":1}
            }),
        };
        self.store.issues.get_mut("NBJ-103").unwrap().fields["parent"] = json!(infra);
        self.parents.details.insert(
            infra.key.clone(),
            (
                Instant::now(),
                Some(ParentDetails {
                    issue: infra.clone(),
                    color: Some("teal".into()),
                }),
            ),
        );
        let neutral = Issue {
            key: "NBJ-109".into(),
            fields: json!({"issuetype":{"name":"Epic"}}),
        };
        self.store.issues.get_mut("NBJ-107").unwrap().fields["parent"] = json!(neutral);
        self.store.tabs[0].push("NBJ-107".into());
    }
}

fn label_color(value: &str) -> Option<Hsla> {
    let value = value.trim().to_ascii_lowercase();
    let base = if let Some(index) = value
        .strip_prefix("ghx-label-")
        .or_else(|| value.strip_prefix("color_"))
    {
        // Atlassian's published Epic Color swatches, in key order.
        const PALETTE: [u32; 14] = [
            0x243859, 0xfa9920, 0xfac304, 0x2a53cc, 0x2aa3bf, 0x58d8a4, 0x8677d9, 0x5244aa,
            0xfa7353, 0x3884ff, 0x34c7e6, 0x6b778c, 0x128759, 0xdd350d,
        ];
        *PALETTE.get(index.parse::<usize>().ok()?.checked_sub(1)?)?
    } else {
        match value.replace([' ', '-'], "_").as_str() {
            "gray" | "grey" => 0x6b778c,
            "dark_gray" | "dark_grey" => 0x243859,
            "blue" => 0x3884ff,
            "dark_blue" => 0x2a53cc,
            "teal" => 0x34c7e6,
            "dark_teal" => 0x2aa3bf,
            "green" => 0x58d8a4,
            "dark_green" => 0x128759,
            "yellow" => 0xfac304,
            "dark_yellow" => 0xfa9920,
            "orange" => 0xfa7353,
            "dark_orange" => 0xdd350d,
            "red" => 0xfa7353,
            "purple" => 0x8677d9,
            "dark_purple" => 0x5244aa,
            "magenta" | "pink" => 0xda62ac,
            _ => {
                let hex = value.strip_prefix('#').filter(|hex| hex.len() == 6)?;
                u32::from_str_radix(hex, 16).ok()?
            }
        }
    };
    // Keep Jira's hue and use a consistent lightness for dark label text.
    let color: Hsla = rgb(base).into();
    Some(hsla(color.h, (color.s * 0.65).min(0.47), 0.65, 1.))
}
