use crate::{
    config::Config,
    jira::{Client, CreatedIssue, Issue},
    picker::{Choice, Picker},
    store::Store,
};
use gpui_kit::base::Disableable;
use gpui_kit::base::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{
    Sizable, TitleBar,
    button::Button,
    button::ButtonVariants,
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc, time::Duration};

type FieldBounds = Rc<RefCell<HashMap<(String, String), Bounds<Pixels>>>>;

struct RelatedView {
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    selected: usize,
    anchor: ScrollAnchor,
}

enum Message {
    ConfigChanged(Result<Config, String>),
    Connected(Result<(Client, Value), String>),
    Tab(usize, u64, Result<Vec<Issue>, String>),
    Detail(u64, Result<Issue, String>),
    Saved(String, Option<Issue>, Result<Value, String>),
    Created(u64, Result<CreatedIssue, String>),
    Choices(u64, Result<Vec<Choice>, String>),
    Related(u64, String, u64, Result<Vec<Issue>, String>),
}
#[derive(Clone)]
struct Sender {
    tx: mpsc::Sender<(u64, Message)>,
    epoch: u64,
}
impl Sender {
    fn send(&self, message: Message) -> Result<(), Box<mpsc::SendError<(u64, Message)>>> {
        self.tx.send((self.epoch, message)).map_err(Box::new)
    }
}
pub struct AppView {
    avatars: crate::avatar::AvatarCache,
    focus: FocusHandle,
    config: Option<Config>,
    client: Option<Client>,
    store: Store,
    inputs: Vec<Entity<InputState>>,
    editor: Entity<zorite_editor::EditorState>,
    summary_input: Entity<InputState>,
    tab: usize,
    selected: usize,
    stack: Vec<String>,
    status: String,
    loading: usize,
    tx: Sender,
    rx: mpsc::Receiver<(u64, Message)>,
    demo: bool,
    edit: Option<(String, String, crate::adf::Document)>,
    picker: Option<Picker>,
    picker_request: u64,
    picker_input: Entity<InputState>,
    picker_scroll: UniformListScrollHandle,
    picker_anchor: Point<Pixels>,
    field_bounds: FieldBounds,
    palette: bool,
    subscriptions: Vec<Subscription>,
    create_summary: String,
    create_request: u64,
    create_pending: Option<u64>,
    create_error: Option<String>,
    create_refocus: bool,
    created_detail_error: Option<(String, String)>,
    saving: bool,
    sort: std::collections::HashMap<usize, (String, u8)>,
    list_scroll: UniformListScrollHandle,
    detail_scroll: ScrollHandle,
    detail_offsets: HashMap<String, Point<Pixels>>,
    related_views: RefCell<HashMap<(String, String), RelatedView>>,
    related_pending: HashMap<String, u64>,
    related_errors: HashMap<String, String>,
    related_request: u64,
    related_navigation: Option<crate::related::Navigation>,
    related_input: Entity<InputState>,
    related_origin: Point<Pixels>,
    related_reveal: bool,
    pending_config: Option<Config>,
    last_refresh: std::time::Instant,
    cache_writer: mpsc::Sender<(Store, Config)>,
    current_user: Option<Value>,
    refresh_errors: Vec<String>,
    revision: u64,
    pending_refresh: bool,
    connecting: bool,
}
impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (raw_tx, rx) = mpsc::channel();
        let tx = Sender {
            tx: raw_tx,
            epoch: 0,
        };
        let preview = std::env::args().any(|a| a == "--demo" || a == "--editor-check");
        let inputs = [
            "https://yourcompany.atlassian.net",
            "you@company.com",
            "",
            "",
            "Filter issues…",
            "Search commands or jump to an issue…",
        ]
        .iter()
        .enumerate()
        .map(|(i, p)| cx.new(|cx| InputState::new(window, cx).placeholder(*p).masked(i == 2)))
        .collect();
        let config = if preview { None } else { Config::load().ok() };
        let store = config.as_ref().map(Store::load).unwrap_or_default();
        let editor = cx.new(|cx| zorite_editor::EditorState::new(window, cx));
        let summary_input = cx.new(|cx| InputState::new(window, cx).placeholder("Issue summary"));
        let picker_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter"));
        let related_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter related issues"));
        let mut this = Self {
            avatars: Default::default(),
            focus: cx.focus_handle(),
            config,
            client: None,
            store,
            inputs,
            editor,
            summary_input,
            tab: 0,
            selected: 0,
            stack: Vec::new(),
            status: String::new(),
            loading: 0,
            tx,
            rx,
            demo: false,
            edit: None,
            picker: None,
            picker_request: 0,
            picker_input,
            picker_scroll: UniformListScrollHandle::new(),
            picker_anchor: point(px(24.), px(120.)),
            field_bounds: Default::default(),
            palette: false,
            sort: Default::default(),
            list_scroll: UniformListScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            detail_offsets: Default::default(),
            related_views: Default::default(),
            related_pending: Default::default(),
            related_errors: Default::default(),
            related_request: 0,
            related_navigation: None,
            related_input,
            related_origin: point(px(0.), px(0.)),
            related_reveal: false,
            pending_config: None,
            last_refresh: std::time::Instant::now(),
            cache_writer: crate::store::writer(),
            current_user: None,
            refresh_errors: Vec::new(),
            revision: 0,
            pending_refresh: false,
            connecting: false,
            subscriptions: Vec::new(),
            create_summary: String::new(),
            create_request: 0,
            create_pending: None,
            create_error: None,
            create_refocus: false,
            created_detail_error: None,
            saving: false,
        };
        if std::env::args().any(|a| a == "--demo") {
            this.demo();
        }
        if std::env::args().any(|a| a == "--editor-check") {
            this.demo();
            this.edit = Some((
                "NBJ-101".into(),
                "description".into(),
                crate::adf::Document::default(),
            ));
            this.editor.update(cx,|e,cx|e.set_text("GPUI Kit + zorite-editor compatibility check\n\nType here to test the native editor.",cx));
        }
        if this.config.is_some() && !this.demo {
            this.connect_existing();
        }
        this.focus.focus(window, cx);
        let watch_tx = this.tx.tx.clone();
        std::thread::spawn(move || {
            let mut modified = std::fs::metadata(crate::config::path())
                .and_then(|m| m.modified())
                .ok();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let next = std::fs::metadata(crate::config::path())
                    .and_then(|m| m.modified())
                    .ok();
                if next != modified {
                    modified = next;
                    if watch_tx
                        .send((
                            u64::MAX,
                            Message::ConfigChanged(Config::load().map_err(|e| e.to_string())),
                        ))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        this.subscriptions
            .push(cx.observe(&this.inputs[4], |_, _, cx| cx.notify()));
        this.subscriptions
            .push(cx.observe(&this.inputs[5], |_, _, cx| cx.notify()));
        this.subscriptions.push(cx.subscribe_in(
            &this.inputs[5],
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.palette_execute(window, cx);
                }
            },
        ));
        this.subscriptions.push(cx.subscribe_in(
            &this.picker_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(picker) = this.picker.as_mut()
                {
                    picker.set_query(this.picker_input.read(cx).value().to_string());
                    this.picker_scroll
                        .scroll_to_item(picker.selected, ScrollStrategy::Nearest);
                    cx.notify();
                }
            },
        ));
        this.subscriptions.push(cx.subscribe_in(
            &this.related_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(navigation) = this.related_navigation.as_mut()
                {
                    navigation.set_query(this.related_input.read(cx).value().to_string());
                    this.related_reveal = true;
                    cx.notify();
                }
            },
        ));
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                if view.update(cx, |view, cx| view.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        this
    }
    fn connect_existing(&mut self) {
        self.connecting = true;
        self.tx.epoch += 1;
        let c = self.config.clone().unwrap();
        let tx = self.tx.clone();
        self.status = "Connecting to Jira…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let token = c.credential()?.get_password()?;
                let client = Client::new(c, token);
                let user = client.get("myself")?;
                Ok((client, user))
            })();
            let _ = tx.send(Message::Connected(result.map_err(|e| e.to_string())));
        });
    }
    fn connect(&mut self, cx: &mut Context<Self>) {
        if self.connecting {
            return;
        }
        let values: Vec<String> = self
            .inputs
            .iter()
            .take(4)
            .map(|s| s.read(cx).value().to_string().trim().to_string())
            .collect();
        let c = Config::initial(values[0].clone(), values[1].clone(), values[3].clone());
        if let Err(e) = c.validate() {
            self.status = e.to_string();
            return;
        }
        if values[2].is_empty() {
            self.status = "Enter an API token".into();
            return;
        }
        let token = values[2].clone();
        self.connecting = true;
        self.tx.epoch += 1;
        let tx = self.tx.clone();
        self.status = "Checking your account…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let mut c = c;
                if let Ok(saved) = Config::load()
                    && saved.jira.base_url == c.jira.base_url
                    && saved.jira.email == c.jira.email
                {
                    c.tabs = saved.tabs;
                    c.cache_ttl = saved.cache_ttl;
                }
                let client = Client::new(c.clone(), token.clone());
                let user = client.get("myself")?;
                c.credential()?.set_password(&token)?;
                c.save()?;
                Ok((client, user))
            })();
            let _ = tx.send(Message::Connected(result.map_err(|e| e.to_string())));
        });
    }
    fn refresh(&mut self) {
        if self.loading > 0 || self.saving {
            self.pending_refresh = true;
            return;
        }
        self.pending_refresh = false;
        if let Some(client) = self.client.clone() {
            self.refresh_errors.clear();
            self.last_refresh = std::time::Instant::now();
            self.loading = client.config.tabs.len();
            self.status = "Refreshing issues…".into();
            for index in 0..self.loading {
                let client = client.clone();
                let tx = self.tx.clone();
                let revision = self.revision;
                std::thread::spawn(move || {
                    let result = client.search(index).map_err(|e| e.to_string());
                    let _ = tx.send(Message::Tab(index, revision, result));
                });
            }
        }
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.avatars.poll();
        while let Ok((epoch, message)) = self.rx.try_recv() {
            if epoch != u64::MAX && epoch != self.tx.epoch {
                continue;
            }
            changed = true;
            match message {
                Message::ConfigChanged(Ok(c)) => {
                    if !self.demo {
                        self.pending_config = Some(c);
                    }
                }
                Message::ConfigChanged(Err(e)) => {
                    self.status = format!("Config reload failed: {e}")
                }
                Message::Connected(Ok((client, user))) => {
                    self.connecting = false;
                    self.current_user = Some(user.clone());
                    self.status = format!(
                        "Connected as {}",
                        user["displayName"].as_str().unwrap_or("you")
                    );
                    self.config = Some(client.config.clone());
                    self.client = Some(client);
                    self.refresh();
                }
                Message::Connected(Err(e)) => {
                    self.connecting = false;
                    self.status = format!("Connection failed: {e}");
                }
                Message::Tab(index, revision, result) => {
                    self.loading = self.loading.saturating_sub(1);
                    if revision != self.revision {
                        self.pending_refresh = true;
                        continue;
                    }
                    match result {
                        Ok(issues) => {
                            if self.saving {
                                continue;
                            }
                            let selected_key = self.keys(cx).get(self.selected).cloned();
                            self.store.set_tab(index, issues);
                            if let Some(key) = selected_key
                                && let Some(pos) = self.keys(cx).iter().position(|k| k == &key)
                            {
                                self.selected = pos;
                            }
                            if let Some(c) = &self.config {
                                let store = self.store.clone();
                                let c = c.clone();
                                let _ = self.cache_writer.send((store, c));
                            }
                            if self.loading == 0 {
                                self.status = if self.refresh_errors.is_empty() {
                                    "Up to date".into()
                                } else {
                                    format!(
                                        "Some tabs could not refresh: {}",
                                        self.refresh_errors.join("; ")
                                    )
                                };
                            }
                        }
                        Err(e) => {
                            self.refresh_errors.push(e.clone());
                            self.status = format!("Refresh failed: {e}");
                        }
                    }
                }
                Message::Detail(revision, Ok(issue)) => {
                    if revision != self.revision {
                        continue;
                    }
                    if self.saving {
                        continue;
                    }
                    if self
                        .created_detail_error
                        .as_ref()
                        .is_some_and(|(key, _)| key == &issue.key)
                    {
                        self.created_detail_error = None;
                    }
                    for reference in crate::related::references(&issue) {
                        self.store.merge_issue(reference);
                    }
                    self.request_related(&issue);
                    self.store.issues.insert(issue.key.clone(), issue);
                }
                Message::Detail(revision, Err(e)) => {
                    if revision == self.revision {
                        self.status = format!("Could not load issue: {e}");
                    }
                }
                Message::Created(request, result) => {
                    if self.create_pending != Some(request) {
                        continue;
                    }
                    self.create_pending = None;
                    self.saving = false;
                    self.create_refocus = true;
                    match result {
                        Ok(created) => {
                            let issue = created.issue;
                            let key = issue.key.clone();
                            self.created_detail_error =
                                created.detail_error.map(|error| (key.clone(), error));
                            for reference in crate::related::references(&issue) {
                                self.store.merge_issue(reference);
                            }
                            self.request_related(&issue);
                            self.store.issues.insert(key.clone(), issue);
                            self.edit = None;
                            self.create_error = None;
                            self.navigate_to(key.clone());
                            self.status = format!("Created {key}");
                            if self.demo {
                                self.store.tabs[self.tab].insert(0, key);
                            } else {
                                self.refresh();
                                if let Some(config) = &self.config {
                                    let _ = self
                                        .cache_writer
                                        .send((self.store.clone(), config.clone()));
                                }
                            }
                        }
                        Err(error) => {
                            self.create_error = Some(error);
                            self.status.clear();
                        }
                    }
                }
                Message::Saved(key, old, result) => {
                    self.saving = false;
                    match result {
                        Ok(data) => {
                            self.edit = None;
                            self.status = "Saved".into();
                            if data["deleted"] == true {
                                self.store.issues.remove(&key);
                                self.stack.retain(|k| k != &key);
                                for tab in &mut self.store.tabs {
                                    tab.retain(|k| k != &key);
                                }
                                self.refresh();
                                continue;
                            }
                            let key = data["key"].as_str().unwrap_or(&key).to_string();
                            self.refresh();
                            if let Some(client) = self.client.clone() {
                                let tx = self.tx.clone();
                                let revision = self.revision;
                                std::thread::spawn(move || {
                                    let _ = tx.send(Message::Detail(
                                        revision,
                                        client.issue(&key).map_err(|e| e.to_string()),
                                    ));
                                });
                            }
                        }
                        Err(e) => {
                            if let Some(old) = old {
                                self.store.issues.insert(key, old);
                            }
                            self.status = format!("Save failed; changes rolled back: {e}");
                        }
                    }
                }
                Message::Related(revision, key, request, result) => {
                    if self.related_pending.get(&key) != Some(&request) {
                        continue;
                    }
                    self.related_pending.remove(&key);
                    if revision != self.revision || self.saving {
                        continue;
                    }
                    match result {
                        Ok(issues) => {
                            self.related_reveal = self.related_navigation.is_some();
                            let children = issues
                                .iter()
                                .filter(|issue| issue.fields["parent"]["key"] == key)
                                .map(|issue| issue.key.clone())
                                .collect();
                            self.store.children.insert(key.clone(), children);
                            for issue in issues {
                                self.store.merge_issue(issue);
                            }
                            self.related_errors.remove(&key);
                            if let Some(config) = &self.config {
                                let _ =
                                    self.cache_writer.send((self.store.clone(), config.clone()));
                            }
                        }
                        Err(error) => {
                            self.related_errors.insert(key, error);
                        }
                    }
                }
                Message::Choices(request, result) => {
                    if let Some(picker) = self.picker.as_mut().filter(|p| p.request == request) {
                        match result {
                            Ok(choices) => {
                                picker.set_choices(choices);
                                self.picker_scroll
                                    .scroll_to_item(picker.selected, ScrollStrategy::Nearest);
                            }
                            Err(error) => {
                                picker.loading = false;
                                picker.error = Some(error);
                            }
                        }
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
        if self.pending_refresh && self.loading == 0 && !self.saving {
            self.refresh();
            cx.notify();
        }
        if !self.demo
            && self.client.is_some()
            && self.last_refresh.elapsed().as_secs()
                >= self
                    .config
                    .as_ref()
                    .and_then(|c| c.ttl_seconds().ok())
                    .unwrap_or(300)
        {
            self.refresh();
            cx.notify();
        }
        if self.loading == 0
            && !self.saving
            && self.edit.is_none()
            && self.picker.is_none()
            && !self.connecting
            && let Some(c) = self.pending_config.take()
        {
            self.store = Store::load(&c);
            self.client = None;
            self.config = Some(c);
            self.tab = 0;
            self.selected = 0;
            self.stack.clear();
            self.detail_offsets.clear();
            self.related_views.borrow_mut().clear();
            self.related_navigation = None;
            self.related_pending.clear();
            self.related_errors.clear();
            self.connect_existing();
            cx.notify();
        }
    }
    fn keys(&self, cx: &App) -> Vec<String> {
        let query = self.inputs[4].read(cx).value().to_string().to_lowercase();
        let mut keys: Vec<String> = self
            .store
            .tabs
            .get(self.tab)
            .into_iter()
            .flatten()
            .filter(|key| {
                self.store.issues.get(*key).is_some_and(|i| {
                    format!("{} {}", i.key, i.field("summary"))
                        .to_lowercase()
                        .contains(&query)
                })
            })
            .cloned()
            .collect();
        let default = self
            .config
            .as_ref()
            .and_then(|c| c.tabs.get(self.tab))
            .map(|t| (t.sort.clone(), 0))
            .unwrap_or_default();
        let (field, mode) = self.sort.get(&self.tab).unwrap_or(&default);
        if !field.is_empty() && *mode < 2 {
            keys.sort_by_key(|k| {
                self.store
                    .issues
                    .get(k)
                    .map(|i| {
                        if field == "key" {
                            i.key.clone()
                        } else {
                            i.field(if field == "type" { "issuetype" } else { field })
                        }
                    })
                    .unwrap_or_default()
            });
            if *mode == 1 {
                keys.reverse();
            }
        }
        keys
    }
    fn open(&mut self, key: String) {
        self.navigate_to(key.clone());
        self.load_issue(key);
    }
    fn navigate_to(&mut self, key: String) {
        if self.related_navigation.take().is_some() {
            self.detail_scroll.set_offset(self.related_origin);
        }
        if let Some(current) = self.stack.last() {
            self.detail_offsets
                .insert(current.clone(), self.detail_scroll.offset());
        }
        self.detail_scroll.set_offset(point(px(0.), px(0.)));
        self.stack.push(key);
    }
    fn load_issue(&self, key: String) {
        if let Some(client) = self.client.clone() {
            let tx = self.tx.clone();
            let revision = self.revision;
            std::thread::spawn(move || {
                let _ = tx.send(Message::Detail(
                    revision,
                    client.issue(&key).map_err(|e| e.to_string()),
                ));
            });
        }
    }
    fn back(&mut self) {
        self.related_navigation = None;
        if let Some(current) = self.stack.pop() {
            self.detail_offsets
                .insert(current, self.detail_scroll.offset());
        }
        let offset = self
            .stack
            .last()
            .and_then(|key| self.detail_offsets.get(key))
            .copied()
            .unwrap_or(point(px(0.), px(0.)));
        self.detail_scroll.set_offset(offset);
    }

    fn request_related(&mut self, issue: &Issue) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.related_request += 1;
        let request = self.related_request;
        let key = issue.key.clone();
        self.related_pending.insert(key.clone(), request);
        self.related_errors.remove(&key);
        let issue = issue.clone();
        let revision = self.revision;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.related(&issue).map_err(|error| error.to_string());
            let _ = tx.send(Message::Related(revision, key, request, result));
        });
    }
    fn current(&self, cx: &App) -> Option<String> {
        self.stack
            .last()
            .cloned()
            .or_else(|| self.keys(cx).get(self.selected).cloned())
    }
    fn begin_edit(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.current(cx) else {
            return;
        };
        let Some(issue) = self.store.issues.get(&key) else {
            return;
        };
        if kind == "description" && issue.fields.get("description").is_none() {
            self.status = "Open the issue and wait for its full description before editing.".into();
            return;
        }
        let doc = if kind == "description" {
            crate::adf::Document::from_adf(&issue.fields[kind])
        } else {
            crate::adf::Document::default()
        };
        let text = if kind == "summary" {
            issue.field(kind)
        } else if kind == "comment" {
            String::new()
        } else {
            doc.markdown.clone()
        };
        self.editor.update(cx, |e, cx| {
            e.set_text(text, cx);
            e.focus(window, cx);
        });
        if kind == "summary" {
            let value = issue.field("summary");
            self.summary_input.update(cx, |s, cx| {
                s.set_value(value, window, cx);
                s.focus(window, cx);
            });
        }
        self.edit = Some((key, kind.into(), doc));
    }
    fn mutate(
        &mut self,
        key: String,
        method: &str,
        path: String,
        body: Value,
        optimistic: Option<Value>,
    ) {
        if self.saving {
            self.status = "Wait for the current change to finish.".into();
            return;
        }
        let Some(client) = self.client.clone() else {
            self.status = "Connect to Jira to save changes. Demo data is read-only.".into();
            return;
        };
        let old = self.store.issues.get(&key).cloned();
        if let Some(fields) = optimistic
            && let Some(issue) = self.store.issues.get_mut(&key)
        {
            for (k, v) in fields.as_object().unwrap() {
                issue.fields[k] = v.clone();
            }
        }
        let tx = self.tx.clone();
        let method = method.to_string();
        self.status = "Saving…".into();
        self.saving = true;
        self.revision += 1;
        std::thread::spawn(move || {
            let result = client
                .send(&method, &path, body)
                .map(|v| {
                    if method == "DELETE" {
                        json!({"deleted":true})
                    } else {
                        v
                    }
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(Message::Saved(key, old, result));
        });
    }
    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((key, kind, doc)) = &self.edit else {
            return;
        };
        let text = if kind == "summary" || kind == "create" {
            self.summary_input.read(cx).value().to_string()
        } else {
            self.editor.read(cx).text().to_string()
        };
        let key = key.clone();
        let kind = kind.clone();
        if self.saving {
            return;
        }
        if kind == "create" {
            if text.trim().is_empty() {
                self.status = "Enter an issue summary".into();
                return;
            }
            let client = self.client.clone();
            if !self.demo && client.is_none() {
                self.status = "Connect an account to create issues".into();
                return;
            }
            let project = self
                .config
                .as_ref()
                .map(|config| config.jira.default_project.clone())
                .unwrap_or_default();
            if project.is_empty() {
                self.status = "Set jira.default_project in your config first".into();
                return;
            }
            self.create_summary = text.trim().to_string();
            self.create_error = None;
            self.start_picker("create", String::new(), window, cx);
            if self.demo {
                self.picker.as_mut().unwrap().set_choices(Choice::from_jira(
                    "create",
                    &json!([
                        {"id":"task","name":"Task","subtask":false},
                        {"id":"story","name":"Story","subtask":false},
                        {"id":"bug","name":"Bug","subtask":false}
                    ]),
                ));
                return;
            }
            let client = client.unwrap();
            let request = self.picker_request;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = (|| -> anyhow::Result<_> {
                    let data = client.get(&format!("issue/createmeta/{project}/issuetypes"))?;
                    Ok(Choice::from_jira("create", &data["issueTypes"]))
                })();
                let _ = tx.send(Message::Choices(request, result.map_err(|e| e.to_string())));
            });
            return;
        }
        if text.trim().is_empty() && kind != "description" {
            self.status = "Text cannot be empty".into();
            return;
        }
        let value = if kind == "summary" {
            json!(text)
        } else {
            match doc.to_adf(&text) {
                Ok(v) => v,
                Err(e) => {
                    self.status = e.to_string();
                    return;
                }
            }
        };
        let (path, body, optimistic) = if kind == "comment" {
            (format!("issue/{key}/comment"), json!({"body":value}), None)
        } else {
            (
                format!("issue/{key}"),
                json!({"fields":{kind.clone():value.clone()}}),
                Some(json!({kind.clone():value})),
            )
        };
        if self.client.is_none() {
            self.status = "Demo data is read-only. Connect your account to save.".into();
            return;
        }
        self.mutate(
            key,
            if kind == "comment" { "POST" } else { "PUT" },
            path,
            body,
            optimistic,
        );
        self.focus.focus(window, cx);
    }
    fn start_picker(
        &mut self,
        kind: &str,
        key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker_request += 1;
        let field = if kind == "done" { "status" } else { kind };
        self.picker_anchor = self
            .field_bounds
            .borrow()
            .get(&(key.clone(), field.into()))
            .map(|bounds| bounds.bottom_left() + point(px(0.), px(4.)))
            .unwrap_or(point(px(24.), px(120.)));
        self.picker = Some(Picker::new(kind, key, self.picker_request));
        if let Some(picker) = self.picker.as_mut()
            && let Some(issue) = self.store.issues.get(&picker.key)
        {
            picker.set_current(issue.field(field), issue.fields[field].clone());
        }
        self.picker_scroll = UniformListScrollHandle::new();
        self.picker_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn choose(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.current(cx) {
            self.choose_for(kind, key, window, cx);
        }
    }

    fn choose_for(&mut self, kind: &str, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if !self.demo && self.client.is_none() {
            self.status = "Connect your account to edit issues.".into();
            return;
        }
        self.start_picker(kind, key.clone(), window, cx);
        if self.demo {
            let data = match kind {
                "status" | "done" => json!([
                    {"id":"11","name":"To Do","to":{"id":"1","name":"To Do","statusCategory":{"key":"new"}}},
                    {"id":"21","name":"In Progress","to":{"id":"2","name":"In Progress","statusCategory":{"key":"indeterminate"}}},
                    {"id":"31","name":"Done","to":{"id":"3","name":"Done","statusCategory":{"key":"done"}}}
                ]),
                "priority" => json!([
                    {"id":"1","name":"Highest"}, {"id":"2","name":"High"},
                    {"id":"3","name":"Medium"}, {"id":"4","name":"Low"}, {"id":"5","name":"Lowest"}
                ]),
                _ => json!([
                    {"accountId":"alex","displayName":"Alex Morgan"},
                    {"accountId":"sam","displayName":"Sam Rivera"},
                    {"accountId":"alexandra","displayName":"Alexandra Morgan-Rivera"},
                    {"accountId":"casey","displayName":"Casey Patel"},
                    {"accountId":"dana","displayName":"Dana Lee"},
                    {"accountId":"jordan","displayName":"Jordan Kim"},
                    {"accountId":"morgan","displayName":"Morgan Chen"},
                    {"accountId":"riley","displayName":"Riley Brooks"},
                    {"accountId":"taylor","displayName":"Taylor Reed"},
                    {"accountId":"avery","displayName":"Avery Lopez"},
                    {"accountId":"cameron","displayName":"Cameron Shaw"},
                    {"accountId":"quinn","displayName":"Quinn Williams"}
                ]),
            };
            self.picker
                .as_mut()
                .unwrap()
                .set_choices(Choice::from_jira(kind, &data));
            self.picker_scroll.scroll_to_item(
                self.picker.as_ref().unwrap().selected,
                ScrollStrategy::Nearest,
            );
            return;
        }
        let client = self.client.clone().unwrap();
        let request = self.picker_request;
        let tx = self.tx.clone();
        let kind = kind.to_string();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Vec<Choice>> {
                let data = if kind == "status" || kind == "done" {
                    client.get(&format!("issue/{key}/transitions"))?["transitions"].clone()
                } else if kind == "priority" {
                    client.get("priority")?
                } else {
                    client.get(&format!(
                        "user/assignable/search?issueKey={key}&maxResults=100"
                    ))?
                };
                Ok(Choice::from_jira(&kind, &data))
            })();
            let _ = tx.send(Message::Choices(request, result.map_err(|e| e.to_string())));
        });
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker = None;
        if self
            .edit
            .as_ref()
            .is_some_and(|(_, kind, _)| kind == "create")
        {
            self.summary_input
                .update(cx, |input, cx| input.focus(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn select_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(choice) = self
            .picker
            .as_ref()
            .and_then(|picker| picker.current())
            .cloned()
        {
            self.apply_choice(choice, cx);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn apply_choice(&mut self, choice: Choice, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        if picker.is_current(&choice) || choice.id.is_empty() {
            return;
        }
        let kind = picker.kind;
        let key = picker.key;
        let id = choice.id;
        if self.demo && ["status", "done", "priority", "assignee"].contains(&kind.as_str()) {
            if let Some(issue) = self.store.issues.get_mut(&key) {
                issue.fields[if kind == "done" { "status" } else { &kind }] = choice.value;
            }
            return;
        }
        if kind == "create" {
            self.begin_create(choice.value);
        } else if kind == "delete" {
            self.mutate(
                key.clone(),
                "DELETE",
                format!("issue/{key}"),
                Value::Null,
                None,
            );
        } else if kind == "status" || kind == "done" {
            self.mutate(
                key.clone(),
                "POST",
                format!("issue/{key}/transitions"),
                json!({"transition":{"id":id}}),
                Some(json!({"status":choice.value})),
            );
        } else {
            let field = if kind == "assignee" {
                json!({"accountId":id})
            } else {
                json!({"id":id})
            };
            self.mutate(
                key.clone(),
                "PUT",
                format!("issue/{key}"),
                json!({"fields":{kind.clone():field}}),
                Some(json!({kind:choice.value})),
            );
        }
        cx.notify();
    }

    fn begin_create(&mut self, issue_type: Value) {
        if self.saving {
            return;
        }
        let client = self.client.clone();
        if !self.demo && client.is_none() {
            self.create_error = Some("Connect an account to create issues".into());
            return;
        }
        self.create_request += 1;
        let request = self.create_request;
        self.create_pending = Some(request);
        self.create_error = None;
        self.saving = true;
        self.revision += 1;
        self.status = "Creating issue…".into();
        let summary = self.create_summary.clone();
        let demo = self.demo;
        let next = self
            .store
            .issues
            .keys()
            .filter_map(|key| key.strip_prefix("NBJ-")?.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = if demo {
                let delay = std::env::args()
                    .find_map(|arg| {
                        arg.strip_prefix("--demo-create-delay=")?
                            .parse::<u64>()
                            .ok()
                    })
                    .unwrap_or(1200)
                    .min(30000);
                std::thread::sleep(Duration::from_millis(delay));
                if request == 1 && std::env::args().any(|arg| arg == "--demo-create-error") {
                    Err("Jira is temporarily unavailable. Try again.".into())
                } else {
                    Ok(CreatedIssue {
                        issue: Issue {
                            key: format!("NBJ-{next}"),
                            fields: json!({"summary":summary,"issuetype":issue_type,"status":{"name":"To Do","statusCategory":{"key":"new"}},"priority":{"name":"Medium"},"assignee":null,"description":null,"comment":{"comments":[]}}),
                        },
                        detail_error: None,
                    })
                }
            } else {
                client
                    .unwrap()
                    .create_issue(&summary, issue_type)
                    .map_err(|error| error.to_string())
            };
            let _ = tx.send(Message::Created(request, result));
        });
    }

    fn picker_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.create_pending.is_some()
            && matches!(event.keystroke.key.as_str(), "escape" | "enter")
        {
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.picker.is_none() && self.related_key(event, window, cx) {
            return;
        }
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match event.keystroke.key.as_str() {
            "escape" => self.close_picker(window, cx),
            "enter" => self.select_picker(window, cx),
            "up" | "down" => {
                picker.move_selection(event.keystroke.key == "down");
                self.picker_scroll
                    .scroll_to_item(picker.selected, ScrollStrategy::Nearest);
                cx.notify();
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }
    fn render_shortcuts(&self, cx: &mut Context<Self>) -> Div {
        let shortcuts = div().flex().items_center().gap_4().flex_shrink_0();
        if self.create_pending.is_some() {
            return shortcuts;
        }
        if self.palette {
            shortcuts
                .child(self.shortcut_button(
                    "footer-run",
                    &["Enter"],
                    "Run or open",
                    "palette-enter",
                    cx,
                ))
                .child(self.shortcut_button("footer-close", &["Esc"], "Close", "escape", cx))
        } else if self.picker.is_some() {
            shortcuts
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(self.shortcut_button("picker-up", &["Up"], "", "picker-up", cx))
                        .child(self.shortcut_button(
                            "picker-down",
                            &["Down"],
                            "Move",
                            "picker-down",
                            cx,
                        )),
                )
                .child(self.shortcut_button(
                    "picker-select",
                    &["Enter"],
                    "Select",
                    "picker-select",
                    cx,
                ))
                .child(self.shortcut_button("picker-close", &["Esc"], "Close", "escape", cx))
        } else if self.related_navigation.is_some() && self.edit.is_none() {
            let selectable = self.related_navigation.as_ref().is_some_and(|navigation| {
                self.store
                    .issues
                    .get(&navigation.root)
                    .is_some_and(|issue| navigation.selected(&self.related_groups(issue)).is_some())
            });
            shortcuts
                .when(selectable, |shortcuts| {
                    shortcuts
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(self.shortcut_button(
                                    "related-up",
                                    &["↑"],
                                    "",
                                    "related-up",
                                    cx,
                                ))
                                .child(self.shortcut_button(
                                    "related-down",
                                    &["↓"],
                                    "Move",
                                    "related-down",
                                    cx,
                                )),
                        )
                        .child(self.shortcut_button(
                            "related-open",
                            &["Enter"],
                            "Open",
                            "related-open",
                            cx,
                        ))
                })
                .child(self.shortcut_button(
                    "related-cancel",
                    &["Esc"],
                    "Cancel",
                    "related-cancel",
                    cx,
                ))
        } else if self.config.is_none() {
            shortcuts
        } else if self.edit.is_some() {
            shortcuts
                .child(self.shortcut_button(
                    "footer-save",
                    &["Ctrl+Enter"],
                    "Save",
                    "save-edit",
                    cx,
                ))
                .child(self.shortcut_button("footer-cancel", &["Esc"], "Cancel", "escape", cx))
        } else {
            shortcuts
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(self.shortcut_button("footer-down", &["J"], "", "j", cx))
                        .child(self.shortcut_button("footer-up", &["K"], "", "k", cx))
                        .child(muted("Move")),
                )
                .child(self.shortcut_button(
                    "footer-open",
                    &["Enter"],
                    if self.stack.is_empty() {
                        "Open"
                    } else {
                        "Related"
                    },
                    "enter",
                    cx,
                ))
                .child(self.shortcut_button("footer-filter", &["/"], "Filter", "/", cx))
                .child(self.shortcut_button("footer-refresh", &["R"], "Refresh", "r", cx))
                .child(self.shortcut_button(
                    "footer-commands",
                    &["Ctrl+K"],
                    "Commands",
                    "palette",
                    cx,
                ))
        }
    }

    fn field_trigger(
        &self,
        kind: &str,
        issue: &Issue,
        content: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = issue.key.clone();
        let field = kind.to_string();
        let bounds_key = (key.clone(), field.clone());
        let bounds = self.field_bounds.clone();
        let open = self
            .picker
            .as_ref()
            .is_some_and(|p| p.kind == kind && p.key == issue.key);
        div()
            .min_w_0()
            .max_w_full()
            .on_children_prepainted(move |children, _, _| {
                if let Some(rect) = children.first() {
                    bounds.borrow_mut().insert(bounds_key.clone(), *rect);
                }
            })
            .child(
                Button::new(SharedString::from(format!("field-{kind}-{}", issue.key)))
                    .ghost()
                    .small()
                    .h(px(24.))
                    .min_w_0()
                    .max_w_full()
                    .px_1()
                    .disabled(self.saving)
                    .when(open, |button| button.bg(rgb(0x242d3a)))
                    .accessibility_label(format!("Change {kind}: {}", issue.field(kind)))
                    .child(content)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this
                            .picker
                            .as_ref()
                            .is_some_and(|p| p.kind == field && p.key == key)
                        {
                            this.close_picker(window, cx);
                        } else {
                            this.choose_for(&field, key.clone(), window, cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn all_related_groups(&self, issue: &Issue) -> Vec<crate::related::Group> {
        crate::related::groups(
            issue,
            &self.store.issues,
            self.store
                .children
                .get(&issue.key)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        )
    }

    fn render_created_detail_error(
        &self,
        issue: &Issue,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (_, error) = self
            .created_detail_error
            .as_ref()
            .filter(|(key, _)| key == &issue.key)?;
        let key = issue.key.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().text_color(rgb(0xf07b84)).child(format!(
                    "Issue created, but details could not be loaded: {error}"
                )))
                .child(
                    Button::new("retry-created-detail")
                        .small()
                        .label("Retry")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_issue(key.clone());
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn related_groups(&self, issue: &Issue) -> Vec<crate::related::Group> {
        let groups = self.all_related_groups(issue);
        if let Some(navigation) = self
            .related_navigation
            .as_ref()
            .filter(|nav| nav.root == issue.key)
        {
            navigation.filter(groups)
        } else {
            groups
        }
    }

    fn start_related_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.stack.last().cloned() else {
            return;
        };
        self.related_origin = self.detail_scroll.offset();
        self.related_navigation = Some(crate::related::Navigation::new(root));
        self.related_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.related_reveal = true;
        cx.notify();
    }

    fn close_related_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.related_navigation = None;
        self.detail_scroll.set_offset(self.related_origin);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn scroll_related_selection(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(navigation) = &self.related_navigation else {
            return;
        };
        let Some(issue) = self.store.issues.get(&navigation.root) else {
            return;
        };
        let groups = self.related_groups(issue);
        let Some((group_id, key)) = navigation.selected(&groups) else {
            return;
        };
        if let Some(view) = self
            .related_views
            .borrow()
            .get(&(navigation.root.clone(), group_id.clone()))
        {
            let index = groups
                .iter()
                .find(|group| group.id == group_id)
                .and_then(|group| group.rows.iter().position(|row| row.key == key))
                .unwrap_or(0);
            view.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
            let bounds = view.scroll.0.borrow().base_handle.bounds();
            let viewport = self.detail_scroll.bounds();
            if bounds.top() < viewport.top() || bounds.bottom() > viewport.bottom() {
                view.anchor.scroll_to(window, cx);
            }
        }
    }

    fn move_related(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(issue) = self.stack.last().and_then(|key| self.store.issues.get(key)) else {
            return;
        };
        let groups = self.related_groups(issue);
        if let Some(navigation) = &mut self.related_navigation {
            navigation.move_selection(&groups, down);
        }
        self.related_reveal = true;
        self.related_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn open_related_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.related_navigation.as_ref().and_then(|navigation| {
            let issue = self.store.issues.get(&navigation.root)?;
            navigation.selected(&self.related_groups(issue))
        });
        if let Some((_, key)) = target {
            self.close_related_navigation(window, cx);
            self.open(key);
            cx.notify();
        }
    }

    fn related_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.related_navigation.is_none()
            || self.palette
            || self.edit.is_some()
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        if event.keystroke.key != "escape"
            && !self
                .related_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_related_navigation(window, cx),
            "enter" => self.open_related_selection(window, cx),
            "up" | "down" => self.move_related(event.keystroke.key == "down", window, cx),
            _ => return false,
        }
        window.prevent_default();
        cx.stop_propagation();
        true
    }

    fn render_related(
        &self,
        issue: &Issue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let groups = self.all_related_groups(issue);
        let key_width = groups
            .iter()
            .flat_map(|group| &group.rows)
            .map(|row| row.key.chars().count() as f32 * 9. + 48.)
            .fold(164., f32::max);
        let navigation = self
            .related_navigation
            .as_ref()
            .filter(|nav| nav.root == issue.key);
        let groups = if let Some(navigation) = navigation {
            navigation.filter(groups)
        } else {
            groups
        };
        let target = navigation.and_then(|nav| nav.selected(&groups));
        let empty = groups.is_empty();
        let error = self.related_errors.get(&issue.key);
        if empty
            && error.is_none()
            && !self.related_pending.contains_key(&issue.key)
            && navigation.is_none()
        {
            return None;
        }
        let mut section = div().flex().flex_col().gap_3();
        for group in groups {
            section = section.child(self.render_related_group(
                &issue.key,
                group,
                key_width,
                target.clone(),
                window,
                cx,
            ));
        }
        if let Some(error) = error {
            let issue = issue.clone();
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(muted(format!("Could not refresh related issues: {error}")))
                    .child(
                        Button::new("retry-related")
                            .small()
                            .label("Retry")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_related(&issue);
                                cx.notify();
                            })),
                    ),
            );
        } else if empty {
            section = section.child(muted(if self.related_pending.contains_key(&issue.key) {
                "Loading related issues…"
            } else if navigation.is_some_and(|nav| !nav.query.trim().is_empty()) {
                "No matches"
            } else {
                "No related issues"
            }));
        }
        Some(section.into_any_element())
    }

    fn render_related_group(
        &self,
        root: &str,
        group: crate::related::Group,
        key_width: f32,
        target: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = (root.to_string(), group.id.clone());
        let (focus, scroll, selected, anchor) = {
            let mut views = self.related_views.borrow_mut();
            let view = views.entry(id.clone()).or_insert_with(|| RelatedView {
                focus: cx.focus_handle(),
                scroll: UniformListScrollHandle::new(),
                selected: 0,
                anchor: ScrollAnchor::for_handle(self.detail_scroll.clone()),
            });
            view.selected = view.selected.min(group.rows.len().saturating_sub(1));
            (
                view.focus.clone(),
                view.scroll.clone(),
                view.selected,
                view.anchor.clone(),
            )
        };
        let focused = focus.is_focused(window);
        let active_group = target
            .as_ref()
            .is_some_and(|(group_id, _)| group_id == &group.id);
        let count = group.rows.len();
        let keys: Vec<_> = group.rows.iter().map(|row| row.key.clone()).collect();
        let row_id = id.clone();
        let title = group.title.clone();
        let visible_rows = ((window.viewport_size().height / px(32.)).floor() as usize)
            .saturating_sub(if self.related_navigation.is_some() {
                11
            } else {
                8
            })
            .clamp(4, 14);
        let rows = uniform_list(
            SharedString::from(format!("related-list-{}-{}", root, group.id)),
            count,
            cx.processor(
                move |this: &mut Self,
                      range: std::ops::Range<usize>,
                      window: &mut Window,
                      cx: &mut Context<Self>| {
                    range
                        .map(|index| {
                            this.render_related_row(
                                &row_id,
                                &group.rows[index],
                                target
                                    .as_ref()
                                    .map(|(group_id, key)| {
                                        group_id == &row_id.1 && key == &group.rows[index].key
                                    })
                                    .unwrap_or(focused && selected == index),
                                key_width,
                                window,
                                cx,
                            )
                        })
                        .collect()
                },
            ),
        )
        .h(px((count.min(visible_rows) * 32) as f32))
        .track_scroll(&scroll);
        div()
            .id(SharedString::from(format!(
                "related-section-{}-{}",
                root, id.1
            )))
            .flex()
            .flex_col()
            .anchor_scroll(Some(anchor))
            .gap_0()
            .child(
                section_heading(&title, count).when(focused || active_group, |heading| {
                    heading.text_color(rgb(0xe6ebf3))
                }),
            )
            .child(
                div()
                    .id(SharedString::from(format!(
                        "related-group-{}-{}",
                        id.0, id.1
                    )))
                    .relative()
                    .when(count > visible_rows, |group| {
                        group.border_y_1().border_color(rgb(0x303744)).pr_3()
                    })
                    .track_focus(&focus)
                    .tab_stop(true)
                    .key_context("RelatedIssues")
                    .role(Role::ListBox)
                    .aria_label(title)
                    .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if this.stack.last() != Some(&id.0) {
                            return;
                        }
                        let key = event.keystroke.key.as_str();
                        if !matches!(
                            key,
                            "j" | "k" | "down" | "up" | "home" | "end" | "enter" | "escape"
                        ) {
                            return;
                        }
                        if key == "escape" {
                            this.focus.focus(window, cx);
                        } else {
                            let target = {
                                let mut views = this.related_views.borrow_mut();
                                let Some(view) = views.get_mut(&id) else {
                                    return;
                                };
                                view.selected = match key {
                                    "j" | "down" => {
                                        (view.selected + 1).min(keys.len().saturating_sub(1))
                                    }
                                    "k" | "up" => view.selected.saturating_sub(1),
                                    "home" => 0,
                                    "end" => keys.len().saturating_sub(1),
                                    _ => view.selected,
                                };
                                view.scroll
                                    .scroll_to_item(view.selected, ScrollStrategy::Nearest);
                                keys.get(view.selected).cloned()
                            };
                            if key == "enter"
                                && let Some(target) = target
                            {
                                this.open(target);
                                this.focus.focus(window, cx);
                            }
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(rows)
                    .when(count > visible_rows, |group| {
                        group.child(Scrollbar::vertical(&scroll).mode(ScrollbarMode::Always))
                    }),
            )
            .into_any_element()
    }

    fn render_related_row(
        &self,
        group: &(String, String),
        issue: &Issue,
        selected: bool,
        key_width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = issue.key.clone();
        let id = format!("related-{}-{}-{}", group.0, group.1, key);
        let summary = issue.fields["summary"]
            .as_str()
            .unwrap_or("Loading…")
            .to_string();
        let mut content = div()
            .w_full()
            .min_w_0()
            .text_size(px(13.))
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(key_width))
                    .pr_3()
                    .flex_shrink_0()
                    .child(self.issue_key(issue, true, window, cx)),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{id}-summary")))
                    .flex_1()
                    .min_w_0()
                    .pr_4()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(rgb(0xe6ebf3))
                    .tooltip({
                        let summary = summary.clone();
                        move |window, cx| Tooltip::new(summary.clone()).build(window, cx)
                    })
                    .child(summary.clone()),
            );
        let mut status = div().w(px(52.)).flex_shrink_0().flex().justify_center();
        if issue.fields.get("status").is_some() {
            status = status.child(crate::status::icon(
                &issue.fields["status"],
                SharedString::from(format!("{id}-status")),
            ));
        }
        content = content.child(status);
        let mut assignee = div()
            .w(px(160.))
            .pr_3()
            .min_w_0()
            .flex_shrink_0()
            .overflow_hidden();
        if issue.fields.get("assignee").is_some() {
            assignee = assignee.child(self.person(
                &issue.fields["assignee"],
                issue.field("assignee"),
                window,
                cx,
            ));
        }
        content = content.child(assignee);
        let mut priority = div().w(px(52.)).flex_shrink_0().flex().justify_center();
        if issue.fields.get("priority").is_some() {
            priority = priority.child(crate::priority::icon(
                issue.field("priority"),
                SharedString::from(format!("{id}-priority")),
            ));
        }
        content = content.child(priority);
        Button::new(SharedString::from(id))
            .ghost()
            .tab_stop(false)
            .w_full()
            .h(px(32.))
            .px_2()
            .text_size(px(13.))
            .rounded_none()
            .bg(rgb(if selected { 0x253348 } else { 0x191c23 }))
            .border_l_2()
            .border_color(rgb(if selected { 0x79aaf9 } else { 0x191c23 }))
            .accessibility_label(format!("Open {key}: {summary}"))
            .child(content)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open(key.clone());
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    fn type_icon(
        &self,
        kind: &Value,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image = kind["iconUrl"]
            .as_str()
            .filter(|address| !address.is_empty())
            .and_then(|address| {
                self.avatars.get_resource(
                    self.config.as_ref().filter(|_| !self.demo),
                    self.client.as_ref(),
                    address,
                )
            })
            .and_then(|image| image.use_render_image(window, cx));
        crate::issue_type::icon(kind, image, id)
    }

    fn issue_key(
        &self,
        issue: &Issue,
        show_type: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(20.))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.))
            .when(show_type, |row| {
                row.child(self.type_icon(
                    &issue.fields["issuetype"],
                    SharedString::from(format!("key-type-{}", issue.key)),
                    window,
                    cx,
                ))
            })
            .child(
                div()
                    .id(SharedString::from(format!("key-label-{}", issue.key)))
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family("JetBrains Mono")
                    .text_color(rgb(0x91b9f7))
                    .tooltip({
                        let key = issue.key.clone();
                        move |window, cx| Tooltip::new(key.clone()).build(window, cx)
                    })
                    .child(issue.key.clone()),
            )
            .into_any_element()
    }

    fn person(
        &self,
        user: &Value,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let unassigned = user.is_null()
            || name.is_empty()
            || name == "—"
            || (user["accountId"].as_str().is_none() && name.eq_ignore_ascii_case("unassigned"));
        let image = if unassigned {
            None
        } else {
            self.avatars
                .get(
                    self.config.as_ref().filter(|_| !self.demo),
                    self.client.as_ref(),
                    user,
                )
                .and_then(|image| image.use_render_image(window, cx))
        };
        let loaded = image.is_some();
        let avatar = if let Some(image) = image {
            img(image).size_full().rounded_full().into_any_element()
        } else {
            gpui_kit::component::Icon::new(if unassigned {
                gpui_kit::assets::IconName::CircleDashed
            } else {
                gpui_kit::assets::IconName::CircleUserRound
            })
            .size(px(16.))
            .text_color(rgb(0x929cad))
            .into_any_element()
        };
        div()
            .h(px(20.))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_color(rgb(if unassigned { 0x929cad } else { 0xa4afc0 }))
            .child(
                div()
                    .size(px(16.))
                    .flex_shrink_0()
                    .rounded_full()
                    .overflow_hidden()
                    .when(loaded, |avatar| {
                        avatar.border_1().border_color(rgb(0x414b5b))
                    })
                    .child(avatar),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(if unassigned {
                        "Unassigned".to_string()
                    } else {
                        name
                    }),
            )
            .into_any_element()
    }

    fn render_choice_row(
        &self,
        visible_index: usize,
        request: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picker = self.picker.as_ref().unwrap();
        let choice = picker.choices[picker.visible[visible_index]].clone();
        let selected = visible_index == picker.selected;
        let current = picker.is_current(&choice);
        let has_icons = matches!(
            picker.kind.as_str(),
            "priority" | "status" | "done" | "assignee" | "create"
        );
        let content = match picker.kind.as_str() {
            "priority" => crate::priority::detail(choice.label.clone()).into_any_element(),
            "status" | "done" => crate::status::detail(&choice.value).into_any_element(),
            "assignee" => self.person(&choice.value, choice.label.clone(), window, cx),
            "create" => div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(self.type_icon(&choice.value, ("choice-type", visible_index), window, cx))
                .child(choice.label.clone())
                .into_any_element(),
            _ => div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(choice.label.clone())
                .into_any_element(),
        };
        Button::new(("picker-choice", visible_index))
            .ghost()
            .w_full()
            .h(px(32.))
            .px_2()
            .when(!has_icons, |button| button.px_3())
            .justify_start()
            .bg(rgb(if selected { 0x2d405c } else { 0x20252e }))
            .accessibility_label(choice.label.clone())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().overflow_hidden().child(content))
                    .when(current, |row| {
                        row.child(
                            gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Check)
                                .size(px(14.))
                                .flex_shrink_0()
                                .text_color(rgb(0x929cad)),
                        )
                    }),
            )
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered
                    && let Some(picker) = this.picker.as_mut()
                    && picker.request == request
                    && picker.selected != visible_index
                {
                    picker.selected = visible_index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.picker.as_ref().is_some_and(|p| p.request == request) {
                    this.apply_choice(choice.clone(), cx);
                    this.focus.focus(window, cx);
                    cx.notify();
                }
            }))
            .into_any_element()
    }

    fn render_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let picker = self.picker.as_ref().unwrap();
        let kind = picker.kind.clone();
        let count = picker.visible.len();
        let request = picker.request;
        let trigger_bounds = self
            .field_bounds
            .borrow()
            .get(&(
                picker.key.clone(),
                if kind == "done" { "status" } else { &kind }.into(),
            ))
            .copied();
        let anchor = self
            .field_bounds
            .borrow()
            .get(&(
                picker.key.clone(),
                if kind == "done" { "status" } else { &kind }.into(),
            ))
            .map(|bounds| bounds.bottom_left() + point(px(0.), px(4.)))
            .unwrap_or(self.picker_anchor);
        let mut popup = div()
            .id("field-picker")
            .w(px(288.))
            .p_1()
            .flex()
            .flex_col()
            .gap_1()
            .bg(rgb(0x20252e))
            .border_1()
            .border_color(rgb(0x414b5b))
            .rounded_md()
            .shadow_lg()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div().p_1().child(
                    Input::new(&self.picker_input)
                        .small()
                        .focus_bordered(false)
                        .border_color(rgb(0x5b8dcc))
                        .rounded_sm()
                        .aria_label(format!(
                            "Filter {}",
                            if kind == "done" { "status" } else { &kind }
                        )),
                ),
            );
        if picker.loading {
            popup = popup.child(div().p_3().child(muted("Loading…")));
        } else if let Some(error) = &picker.error {
            popup = popup.child(div().p_3().text_color(rgb(0xf07b84)).child(error.clone()));
        } else if count == 0 {
            popup = popup.child(div().p_3().child(muted("No matches")));
        } else {
            popup = popup.child(
                uniform_list(
                    "picker-options",
                    count,
                    cx.processor(
                        move |this: &mut Self,
                              range: std::ops::Range<usize>,
                              window: &mut Window,
                              cx: &mut Context<Self>| {
                            range
                                .map(|visible_index| {
                                    this.render_choice_row(visible_index, request, window, cx)
                                })
                                .collect()
                        },
                    ),
                )
                .h(px((count.min(8) * 32) as f32))
                .track_scroll(&self.picker_scroll),
            );
        }
        let position = if let Some(bounds) = trigger_bounds {
            anchored()
                .attach(bounds, Anchor::BottomLeft)
                .offset(point(px(0.), px(4.)))
        } else {
            anchored().position(anchor)
        };
        deferred(position.snap_to_window_with_margin(px(8.)).child(popup))
            .with_priority(100)
            .into_any_element()
    }

    fn demo(&mut self) {
        self.tx.epoch += 1;
        self.connecting = false;
        self.client = None;
        self.current_user = None;
        self.loading = 0;
        self.pending_config = None;
        self.pending_refresh = false;
        self.store = Store::default();
        self.stack.clear();
        self.detail_offsets.clear();
        self.related_views.borrow_mut().clear();
        self.related_navigation = None;
        self.related_pending.clear();
        self.related_errors.clear();
        self.tab = 0;
        self.selected = 0;
        self.demo = true;
        self.config = Some(Config::initial(
            "https://example.atlassian.net".into(),
            "demo@example.com".into(),
            "NBJ".into(),
        ));
        let issues=[("NBJ-101","Build the native issue list","In Progress","High","Alex Morgan"),("NBJ-102","Add keyboard navigation","To Do","Medium","Sam Rivera"),("NBJ-103","Cache recent issues for instant startup","Done","Low","Alex Morgan"),("NBJ-104","Preserve rich text, inline attachments, and unsupported Jira content when editing descriptions in Markdown","To Do","Highest","Alexandra Morgan-Rivera"),("PLATFORM-12345","Review empty states","To Do","Lowest","Unassigned")].iter().enumerate().map(|(i,(key,summary,status,priority,assignee))|Issue{key:(*key).into(),fields:json!({"summary":summary,"status":{"name":status},"priority":{"name":priority},"issuetype":{"name":(["Task","Story","Bug","Epic","Sub-task"][i]),"subtask":i==4},"assignee":{"displayName":assignee},"description":null})}).collect();
        self.store.set_tab(0, issues);
        self.demo_related();
        self.status = "Demo workspace · sample data".into();
    }
    fn demo_related(&mut self) {
        for (key, summary, kind, status, priority, assignee) in [
            (
                "NBJ-106",
                "Keep keyboard focus inside related groups",
                "Sub-task",
                "In Progress",
                "Medium",
                "Sam Rivera",
            ),
            (
                "NBJ-107",
                "Document custom relationship directions without changing their original meaning",
                "Task",
                "To Do",
                "Low",
                "Alex Morgan",
            ),
        ] {
            self.store.merge_issue(Issue { key: key.into(), fields: json!({"summary":summary,"issuetype":{"name":kind,"subtask":kind=="Sub-task"},"status":{"name":status},"priority":{"name":priority},"assignee":{"displayName":assignee},"description":null}) });
        }
        for (index, summary) in [
            "Define the issue row component",
            "Wire mouse navigation",
            "Preserve scroll on Back",
            "Show directional link groups",
            "Load child issue metadata",
            "Cache linked issue previews",
            "Handle missing linked issues",
            "Keep large epics responsive",
            "Verify keyboard navigation",
        ]
        .iter()
        .enumerate()
        {
            let key = format!("NBJ-{}", 201 + index);
            self.store.merge_issue(Issue { key, fields: json!({"summary":summary,"issuetype":{"name":"Task"},"status":{"name":"To Do"},"priority":{"name":"Medium"},"assignee":{"displayName":"Sam Rivera"},"parent":{"key":"NBJ-104"},"description":null}) });
        }
        let refs: HashMap<String, Value> = self
            .store
            .issues
            .values()
            .map(|issue| {
                let fields = ["summary", "issuetype", "status", "priority", "assignee"]
                    .into_iter()
                    .filter_map(|field| {
                        issue
                            .fields
                            .get(field)
                            .map(|value| (field.to_string(), value.clone()))
                    })
                    .collect::<serde_json::Map<String, Value>>();
                (issue.key.clone(), json!({"key":issue.key,"fields":fields}))
            })
            .collect();
        let root = self.store.issues.get_mut("NBJ-101").unwrap();
        root.fields["parent"] = refs["NBJ-104"].clone();
        root.fields["subtasks"] = json!([refs["PLATFORM-12345"], refs["NBJ-106"]]);
        root.fields["issuelinks"] = json!([
            {"type":{"inward":"is blocked by","outward":"blocks"},"inwardIssue":refs["NBJ-103"]},
            {"type":{"inward":"is blocked by","outward":"blocks"},"outwardIssue":refs["NBJ-102"]},
            {"type":{"inward":"relates to","outward":"relates to"},"outwardIssue":refs["NBJ-107"]}
        ]);
        root.fields["comment"] = json!({"comments":[{"author":{"displayName":"Sam Rivera"},"body":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Navigation check complete."}]}]}}]});
        for key in ["PLATFORM-12345", "NBJ-106"] {
            self.store.issues.get_mut(key).unwrap().fields["parent"] = refs["NBJ-101"].clone();
        }
        for key in ["NBJ-102", "NBJ-103", "NBJ-107"] {
            self.store.issues.get_mut(key).unwrap().fields["parent"] = refs["NBJ-104"].clone();
        }
        self.store.issues.get_mut("NBJ-102").unwrap().fields["issuelinks"] = json!([
            {"type":{"inward":"is blocked by","outward":"blocks"},"inwardIssue":refs["NBJ-101"]}
        ]);
        self.store.children.insert(
            "NBJ-101".into(),
            vec!["PLATFORM-12345".into(), "NBJ-106".into()],
        );
        self.store.children.insert(
            "NBJ-104".into(),
            ["NBJ-101", "NBJ-102", "NBJ-103", "NBJ-107"]
                .into_iter()
                .map(String::from)
                .chain((201..210).map(|number| format!("NBJ-{number}")))
                .collect(),
        );
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.create_pending.is_some() {
            return;
        }
        if self.edit.is_some()
            && event.keystroke.modifiers.control
            && event.keystroke.key == "enter"
        {
            self.save_edit(window, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.picker.is_some() {
            return;
        }
        if event.keystroke.modifiers.control && event.keystroke.key == "k" {
            if self.edit.is_some() {
                return;
            }
            self.palette = !self.palette;
            if self.palette {
                self.inputs[5].update(cx, |s, cx| {
                    s.set_value("", window, cx);
                    s.focus(window, cx);
                });
            } else {
                self.focus.focus(window, cx);
            }
            cx.notify();
            return;
        }
        if event.keystroke.key == "escape" {
            if self.palette {
                self.palette = false;
                self.focus.focus(window, cx);
                cx.notify();
                return;
            }
            if self.edit.take().is_some() || self.picker.take().is_some() {
                self.focus.focus(window, cx);
                cx.notify();
                return;
            }
            self.focus.focus(window, cx);
        }
        if self.edit.is_some() {
            return;
        }
        if !self.focus.is_focused(window) {}
    }
    fn command(
        &mut self,
        action: &crate::actions::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit.is_some() || self.picker.is_some() || !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.run_command(&action.key, window, cx);
    }
    fn run_command(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = false;
        self.related_navigation = None;
        self.focus.focus(window, cx);
        if self.saving && ["s", "p", "a", "t", "e", "m", "c", "delete", "d", "i"].contains(&command)
        {
            self.status = "Wait for the current change to finish.".into();
            cx.notify();
            return;
        }
        if let Some(n) = command
            .strip_prefix("sort-")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(field) = self
                .config
                .as_ref()
                .and_then(|c| c.tabs.get(self.tab))
                .and_then(|t| t.columns.get(n.saturating_sub(1)))
                .cloned()
            {
                self.cycle_sort(field);
            }
            cx.notify();
            return;
        }
        if !self.stack.is_empty() {
            match command {
                "j" | "down" => {
                    let p = self.detail_scroll.offset();
                    self.detail_scroll.set_offset(point(p.x, p.y - px(40.)));
                    cx.notify();
                    return;
                }
                "k" | "up" => {
                    let p = self.detail_scroll.offset();
                    self.detail_scroll.set_offset(point(p.x, p.y + px(40.)));
                    cx.notify();
                    return;
                }
                "home" => {
                    self.detail_scroll.set_offset(point(px(0.), px(0.)));
                    cx.notify();
                    return;
                }
                "enter" | "/" => {
                    self.start_related_navigation(window, cx);
                    return;
                }
                _ => {}
            }
        }
        let keys = self.keys(cx);
        let n = keys.len();
        match command {
            "j" | "down" => self.selected = (self.selected + 1).min(n.saturating_sub(1)),
            "k" | "up" => self.selected = self.selected.saturating_sub(1),
            "home" => self.selected = 0,
            "end" => self.selected = n.saturating_sub(1),
            "enter" => {
                if let Some(key) = keys.get(self.selected) {
                    self.open(key.clone());
                }
            }
            "escape" => {
                if self.stack.is_empty() {
                    self.inputs[4].update(cx, |s, cx| s.set_value("", window, cx));
                } else {
                    self.back();
                }
            }
            "r" => self.refresh(),
            "q" => cx.quit(),
            "h" | "left" => {
                self.tab = self.tab.saturating_sub(1);
                self.selected = 0;
            }
            "l" | "right" => {
                self.tab = (self.tab + 1).min(
                    self.config
                        .as_ref()
                        .map(|c| c.tabs.len().saturating_sub(1))
                        .unwrap_or(0),
                );
                self.selected = 0;
            }
            "/" => {
                self.inputs[4].update(cx, |s, cx| s.focus(window, cx));
            }
            "t" => self.begin_edit("summary", window, cx),
            "e" => self.begin_edit("description", window, cx),
            "m" => self.begin_edit("comment", window, cx),
            "s" => self.choose("status", window, cx),
            "p" => self.choose("priority", window, cx),
            "a" => self.choose("assignee", window, cx),
            "ctrl-," => {
                cx.open_url(&format!(
                    "file:///{}",
                    crate::config::path().to_string_lossy().replace('\\', "/")
                ));
            }
            "ctrl-shift-c" => {
                if self.saving {
                    return;
                }
                if let Some(c) = &self.config {
                    for (i, value) in [
                        c.jira.base_url.clone(),
                        c.jira.email.clone(),
                        String::new(),
                        c.jira.default_project.clone(),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        self.inputs[i].update(cx, |s, cx| s.set_value(value, window, cx));
                    }
                }
                self.tx.epoch += 1;
                self.loading = 0;
                self.connecting = false;
                self.pending_refresh = false;
                self.pending_config = None;
                self.current_user = None;
                self.client = None;
                self.config = None;
                self.demo = false;
                self.store = Store::default();
                self.stack.clear();
                self.detail_offsets.clear();
                self.related_views.borrow_mut().clear();
                self.related_navigation = None;
                self.related_pending.clear();
                self.related_errors.clear();
                self.status.clear();
            }
            "delete" => {
                if let Some(key) = self.current(cx) {
                    self.start_picker("delete", key.clone(), window, cx);
                    self.picker
                        .as_mut()
                        .unwrap()
                        .set_choices(vec![Choice::named(&key, "Delete permanently")]);
                }
            }
            "i" => {
                if let (Some(key), Some(user)) = (self.current(cx), self.current_user.clone()) {
                    self.mutate(
                        key.clone(),
                        "PUT",
                        format!("issue/{key}/assignee"),
                        json!({"accountId":user["accountId"]}),
                        Some(json!({"assignee":user})),
                    );
                } else {
                    self.status = "Connect your account to assign issues".into();
                }
            }
            "d" => {
                self.choose("done", window, cx);
                self.status = "Choose the workflow's Done transition".into();
            }
            "c" => {
                self.create_error = None;
                self.edit = Some((
                    "New issue".into(),
                    "create".into(),
                    crate::adf::Document::default(),
                ));
                self.summary_input.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                    s.focus(window, cx);
                });
            }
            "y" | "u" | "o" => {
                let key = self
                    .stack
                    .last()
                    .cloned()
                    .or_else(|| keys.get(self.selected).cloned());
                if let Some(key) = key {
                    let url = format!(
                        "{}/browse/{}",
                        self.config.as_ref().unwrap().jira.base_url,
                        key
                    );
                    match command {
                        "y" => cx.write_to_clipboard(ClipboardItem::new_string(key)),
                        "u" => cx.write_to_clipboard(ClipboardItem::new_string(url)),
                        _ => cx.open_url(&url),
                    }
                }
            }
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                let t = d.parse::<usize>().unwrap_or(1).saturating_sub(1);
                if t < self.config.as_ref().map(|c| c.tabs.len()).unwrap_or(0) {
                    self.tab = t;
                    self.selected = 0;
                }
            }
            _ => {}
        }
        if n > 0 {
            self.list_scroll
                .scroll_to_item(self.selected.min(n - 1), ScrollStrategy::Top);
        }
        cx.notify();
    }
    fn cycle_sort(&mut self, field: String) {
        let mode = self
            .sort
            .get(&self.tab)
            .filter(|(f, _)| f == &field)
            .map(|(_, m)| (m + 1) % 3)
            .unwrap_or(0);
        self.sort.insert(self.tab, (field, mode));
        self.selected = 0;
    }
    fn palette_execute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.inputs[5].read(cx).value().to_string();
        let query = query.trim();
        let candidate = query.to_uppercase();
        let valid_key = candidate.rsplit_once('-').is_some_and(|(p, n)| {
            !p.is_empty()
                && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !n.is_empty()
                && n.chars().all(|c| c.is_ascii_digit())
        });
        if valid_key {
            self.palette = false;
            self.open(candidate);
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if let Some((key, _)) = crate::actions::COMMANDS
            .iter()
            .find(|(_, label)| fuzzy(query, label))
        {
            let key = key.to_string();
            self.run_command(&key, window, cx);
            return;
        }
        if let Some(issue) = self
            .store
            .issues
            .values()
            .find(|i| fuzzy(query, &format!("{} {}", i.key, i.field("summary"))))
        {
            let key = issue.key.clone();
            self.palette = false;
            self.open(key);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }
}
fn fuzzy(query: &str, text: &str) -> bool {
    let lower = text.to_lowercase();
    let mut chars = lower.chars();
    query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .all(|q| chars.by_ref().any(|c| c == q))
}
fn cell(text: String, field: &str) -> Div {
    if matches!(field, "status" | "priority" | "type" | "issuetype") {
        div()
            .w(px(64.))
            .pr_3()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(text)
    } else if field == "summary" {
        div()
            .flex_1()
            .min_w_0()
            .pr_4()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(text)
    } else {
        column(
            text,
            match field {
                "key" => 148.,
                "assignee" | "reporter" => 160.,
                _ => 130.,
            },
        )
        .pr_3()
    }
}
fn section_heading(title: &str, count: usize) -> Div {
    div()
        .h(px(20.))
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(11.))
        .text_color(rgb(0x929cad))
        .child(title.to_uppercase())
        .child(div().text_color(rgb(0x7d8797)).child(count.to_string()))
}
fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_color(rgb(0x929cad))
        .text_size(px(12.))
        .child(text.into())
}
fn column(text: String, width: f32) -> Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .overflow_hidden()
        .child(text)
}
fn adf_text(v: &Value) -> String {
    if v["type"] == "text" {
        return v["text"].as_str().unwrap_or("").into();
    }
    let mut s = v["content"]
        .as_array()
        .map(|a| a.iter().map(adf_text).collect::<String>())
        .unwrap_or_default();
    if matches!(
        v["type"].as_str(),
        Some("paragraph" | "heading" | "listItem")
    ) {
        s.push('\n');
    }
    s
}
impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div().flex().flex_col().flex_1().min_h_0().overflow_hidden();
        if let Some(c) = &self.config {
            let tabs = div()
                .flex()
                .items_center()
                .h(px(40.))
                .flex_shrink_0()
                .px_6()
                .gap_6()
                .border_b_1()
                .border_color(rgb(0x2b303b))
                .children(c.tabs.iter().enumerate().map(|(i, t)| {
                    div()
                        .id(("tab", i))
                        .h_full()
                        .flex()
                        .items_center()
                        .border_b_2()
                        .border_color(rgb(if i == self.tab { 0x79aaf9 } else { 0x191c23 }))
                        .text_color(rgb(if i == self.tab { 0xf0f3f9 } else { 0x929cad }))
                        .cursor_pointer()
                        .child(t.label.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tab = i;
                            this.selected = 0;
                            this.stack.clear();
                            this.detail_offsets.clear();
                            this.related_views.borrow_mut().clear();
                            this.related_navigation = None;
                            this.related_pending.clear();
                            this.related_errors.clear();
                            cx.notify();
                        }))
                }));
            body = body.child(tabs);
            if self
                .related_navigation
                .as_ref()
                .is_some_and(|nav| self.stack.last() == Some(&nav.root))
            {
                body = body.child(
                    div()
                        .id("related-filter")
                        .px_6()
                        .py_2()
                        .flex_shrink_0()
                        .bg(rgb(0x191c23))
                        .border_b_1()
                        .border_color(rgb(0x303744))
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div().max_w(px(480.)).child(
                                Input::new(&self.related_input)
                                    .small()
                                    .focus_bordered(false)
                                    .border_color(rgb(0x79aaf9))
                                    .rounded_sm()
                                    .aria_label("Filter related issues")
                                    .prefix(
                                        gpui_kit::component::Icon::new(
                                            gpui_kit::assets::IconName::Search,
                                        )
                                        .size(px(14.))
                                        .text_color(rgb(0x929cad)),
                                    ),
                            ),
                        ),
                );
            }
            if let Some(key) = self.stack.last() {
                let issue = self.store.issues.get(key).cloned().unwrap_or_default();
                let back_label = self
                    .stack
                    .iter()
                    .rev()
                    .nth(1)
                    .map(|key| format!("Back to {key} (Esc)"))
                    .unwrap_or_else(|| "Back to issues (Esc)".into());
                body = body.child(
                    div()
                        .id("detail")
                        .flex_1()
                        .min_h_0()
                        .track_scroll(&self.detail_scroll)
                        .overflow_y_scroll()
                        .px_6()
                        .py_4()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .pb_3()
                                .border_b_1()
                                .border_color(rgb(0x303744))
                                .child(
                                    div()
                                        .h(px(32.))
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .child(
                                            Button::new("back")
                                                .small()
                                                .icon(gpui_kit::assets::IconName::ArrowLeft)
                                                .accessibility_label(back_label.clone())
                                                .tooltip(back_label)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.back();
                                                    this.focus.focus(window, cx);
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .items_center()
                                                .gap_3()
                                                .child(div().flex_shrink_0().child(
                                                    self.issue_key(&issue, true, window, cx),
                                                ))
                                                .child(
                                                    div()
                                                        .id("detail-title")
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_size(px(20.))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .text_ellipsis()
                                                        .tooltip({
                                                            let summary = issue.field("summary");
                                                            move |window, cx| {
                                                                Tooltip::new(summary.clone())
                                                                    .build(window, cx)
                                                            }
                                                        })
                                                        .child(issue.field("summary")),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .h(px(24.))
                                        .flex()
                                        .items_center()
                                        .gap_5()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(
                                            self.field_trigger(
                                                "status",
                                                &issue,
                                                crate::status::detail(&issue.fields["status"])
                                                    .into_any_element(),
                                                cx,
                                            ),
                                        )
                                        .child(
                                            self.field_trigger(
                                                "priority",
                                                &issue,
                                                crate::priority::detail(issue.field("priority"))
                                                    .into_any_element(),
                                                cx,
                                            ),
                                        )
                                        .child(self.field_trigger(
                                            "assignee",
                                            &issue,
                                            self.person(
                                                &issue.fields["assignee"],
                                                issue.field("assignee"),
                                                window,
                                                cx,
                                            ),
                                            cx,
                                        )),
                                ),
                        )
                        .children(self.render_created_detail_error(&issue, cx))
                        .children(
                            (!crate::adf::Document::display(&issue.fields["description"])
                                .trim()
                                .is_empty())
                            .then(|| {
                                div()
                                    .text_size(px(15.))
                                    .when(self.related_navigation.is_some(), |context| {
                                        context.opacity(0.6)
                                    })
                                    .child(gpui_kit::component::text::TextView::markdown(
                                        "description",
                                        crate::adf::Document::display(&issue.fields["description"]),
                                    ))
                            }),
                        )
                        .children(self.render_related(&issue, window, cx))
                        .children(
                            issue.fields["comment"]["comments"]
                                .as_array()
                                .filter(|comments| !comments.is_empty())
                                .map(|comments| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .when(self.related_navigation.is_some(), |context| {
                                            context.opacity(0.6)
                                        })
                                        .child(section_heading("Comments", comments.len()))
                                        .children(comments.iter().map(|v| {
                                            div()
                                                .py_2()
                                                .border_b_1()
                                                .border_color(rgb(0x303744))
                                                .child(muted(
                                                    v["author"]["displayName"]
                                                        .as_str()
                                                        .unwrap_or("Unknown")
                                                        .to_string(),
                                                ))
                                                .child(adf_text(&v["body"]))
                                        }))
                                }),
                        ),
                );
            } else {
                let keys = self.keys(cx);
                let count = keys.len();
                let columns = c.tabs[self.tab].columns.clone();
                body = body
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(div().w(px(360.)).child(Input::new(&self.inputs[4]).small()))
                            .child(muted(format!("{count} issues"))),
                    )
                    .child(
                        div()
                            .flex()
                            .px_6()
                            .py_2()
                            .text_color(rgb(0x929cad))
                            .text_size(px(11.))
                            .border_l_2()
                            .border_color(rgb(0x191c23))
                            .children(columns.iter().enumerate().map(|(i, field)| {
                                let field = field.clone();
                                cell(field.to_uppercase(), &field)
                                    .id(("column", i))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.cycle_sort(field.clone());
                                        cx.notify();
                                    }))
                            })),
                    );
                if keys.is_empty() {
                    body = body.child(div().flex_1().flex().items_center().justify_center().child(
                        muted(if self.loading > 0 {
                            "Loading your issues…"
                        } else {
                            "No issues here. Try refreshing or changing your filter."
                        }),
                    ));
                } else {
                    body = body.child(
                        uniform_list(
                            "issues",
                            count,
                            cx.processor(
                                move |this: &mut Self,
                                      range: std::ops::Range<usize>,
                                      window: &mut Window,
                                      cx: &mut Context<Self>| {
                                    range
                                        .map(|i| {
                                            let issue = this.store.issues.get(&keys[i]).unwrap();
                                            let selected = i == this.selected;
                                            let key = issue.key.clone();
                                            let weak = cx.entity().downgrade();
                                            div()
                                                .id(("issue", i))
                                                .w_full()
                                                .h(px(32.))
                                                .flex()
                                                .items_center()
                                                .px_6()
                                                .border_l_2()
                                                .border_color(rgb(if selected {
                                                    0x79aaf9
                                                } else {
                                                    0x191c23
                                                }))
                                                .bg(rgb(if selected { 0x253348 } else { 0x191c23 }))
                                                .cursor_pointer()
                                                .children(columns.iter().map(|field| {
                                                    if field == "key" {
                                                        let show_type =
                                                            !columns.iter().any(|field| {
                                                                matches!(
                                                                    field.as_str(),
                                                                    "type" | "issuetype"
                                                                )
                                                            });
                                                        return div()
                                                            .w(px(148.))
                                                            .pr_3()
                                                            .flex_shrink_0()
                                                            .min_w_0()
                                                            .child(this.issue_key(
                                                                issue, show_type, window, cx,
                                                            ))
                                                            .into_any_element();
                                                    }
                                                    if field == "type" || field == "issuetype" {
                                                        return div()
                                                            .w(px(64.))
                                                            .pr_3()
                                                            .flex_shrink_0()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(this.type_icon(
                                                                &issue.fields["issuetype"],
                                                                SharedString::from(format!(
                                                                    "column-type-{}",
                                                                    issue.key
                                                                )),
                                                                window,
                                                                cx,
                                                            ))
                                                            .into_any_element();
                                                    }
                                                    if field == "status" || field == "priority" {
                                                        let icon = if field == "status" {
                                                            crate::status::icon(
                                                                &issue.fields[field],
                                                                ("status", i),
                                                            )
                                                            .into_any_element()
                                                        } else {
                                                            crate::priority::icon(
                                                                issue.field(field),
                                                                ("priority", i),
                                                            )
                                                            .into_any_element()
                                                        };
                                                        return div()
                                                            .w(px(64.))
                                                            .pr_3()
                                                            .flex_shrink_0()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(this.field_trigger(
                                                                field, issue, icon, cx,
                                                            ))
                                                            .into_any_element();
                                                    }
                                                    if field == "assignee" {
                                                        return div()
                                                            .w(px(160.))
                                                            .pr_3()
                                                            .flex_shrink_0()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_hidden()
                                                            .child(this.field_trigger(
                                                                field,
                                                                issue,
                                                                this.person(
                                                                    &issue.fields[field],
                                                                    issue.field(field),
                                                                    window,
                                                                    cx,
                                                                ),
                                                                cx,
                                                            ))
                                                            .into_any_element();
                                                    }
                                                    let value = if field == "key" {
                                                        issue.key.clone()
                                                    } else {
                                                        issue.field(if field == "type" {
                                                            "issuetype"
                                                        } else {
                                                            field
                                                        })
                                                    };
                                                    let color = match field.as_str() {
                                                        "key" => 0x91b9f7,
                                                        "summary" => 0xe6ebf3,
                                                        _ => 0xa4afc0,
                                                    };
                                                    cell(value, field)
                                                        .font_family(if field == "key" {
                                                            "JetBrains Mono"
                                                        } else {
                                                            "Inter"
                                                        })
                                                        .text_color(rgb(color))
                                                        .into_any_element()
                                                }))
                                                .on_click(cx.listener(
                                                    move |this, event: &ClickEvent, _, cx| {
                                                        this.selected = i;
                                                        if event.click_count() == 2 {
                                                            this.open(key.clone());
                                                        }
                                                        cx.notify();
                                                    },
                                                ))
                                                .context_menu(move |mut menu, _, _| {
                                                    for (command, label) in crate::actions::COMMANDS
                                                    {
                                                        let command = command.to_string();
                                                        let weak = weak.clone();
                                                        menu = menu.item(
                                                            PopupMenuItem::new(format!(
                                                                "{label}    {command}"
                                                            ))
                                                            .on_click(move |_, window, cx| {
                                                                let _ =
                                                                    weak.update(cx, |this, cx| {
                                                                        this.selected = i;
                                                                        this.run_command(
                                                                            &command, window, cx,
                                                                        );
                                                                        cx.notify();
                                                                    });
                                                            }),
                                                        );
                                                    }
                                                    menu
                                                })
                                        })
                                        .collect()
                                },
                            ),
                        )
                        .track_scroll(&self.list_scroll)
                        .w_full()
                        .flex_1()
                        .min_h_0(),
                    );
                }
            }
        } else {
            let form = div()
                .w(px(440.))
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div()
                        .text_size(px(22.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Connect to Jira"),
                )
                .children(
                    [
                        "Jira site",
                        "Email address",
                        "API token",
                        "Default project (optional)",
                    ]
                    .iter()
                    .enumerate()
                    .map(|(i, label)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(muted(*label))
                            .child(Input::new(&self.inputs[i]).aria_label(*label))
                    }),
                )
                .child(
                    Button::new("connect")
                        .primary()
                        .bg(rgb(0x3776c8))
                        .text_color(rgb(0xffffff))
                        .label("Connect account")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.connect(cx);
                            cx.notify();
                        })),
                );
            body = body.items_center().justify_center().child(form);
        }
        if self.palette {
            let query = self.inputs[5].read(cx).value().to_string();
            let first = crate::actions::COMMANDS
                .iter()
                .position(|(_, label)| fuzzy(&query, label));
            let results = div()
                .id("palette")
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(muted("COMMANDS"))
                .children(
                    crate::actions::COMMANDS
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, label))| fuzzy(&query, label))
                        .map(|(i, (key, label))| {
                            let key = key.to_string();
                            command_row(
                                ("command", i),
                                label.to_string(),
                                key_name(&key),
                                first == Some(i),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.run_command(&key, window, cx);
                                    cx.notify();
                                },
                            ))
                        }),
                )
                .child(muted("ISSUES"))
                .children(
                    self.store
                        .issues
                        .values()
                        .filter(|i| fuzzy(&query, &format!("{} {}", i.key, i.field("summary"))))
                        .take(30)
                        .enumerate()
                        .map(|(i, issue)| {
                            let key = issue.key.clone();
                            command_row(
                                ("jump", i),
                                issue.field("summary"),
                                key.clone(),
                                first.is_none() && i == 0,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.palette = false;
                                    this.open(key.clone());
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                },
                            ))
                        }),
                );
            body = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full()
                .max_w(px(840.))
                .mx_auto()
                .my_4()
                .p_4()
                .gap_3()
                .bg(rgb(0x20252e))
                .border_1()
                .border_color(rgb(0x394352))
                .rounded_lg()
                .child(div().text_size(px(22.)).child("Commands"))
                .child(Input::new(&self.inputs[5]).small())
                .child(results);
        } else if let Some((key, kind, document)) = &self.edit {
            let single = kind == "summary" || kind == "create";
            let editor_area = if single {
                div()
                    .child(
                        Input::new(&self.summary_input)
                            .disabled(self.saving || self.picker.is_some()),
                    )
                    .into_any_element()
            } else {
                div()
                    .id("markdown-editor")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_4()
                    .bg(rgb(0x14171d))
                    .font_family("JetBrains Mono")
                    .text_size(px(14.))
                    .border_1()
                    .border_color(rgb(
                        if self.editor.read(cx).focus_handle(cx).is_focused(window) {
                            0x79aaf9
                        } else {
                            0x394352
                        },
                    ))
                    .rounded_md()
                    .child(self.editor.clone())
                    .into_any_element()
            };

            body =
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .p_6()
                    .gap_4()
                    .child(muted(if kind == "create" {
                        "New issue".into()
                    } else {
                        format!("{key} / Edit {kind}")
                    }))
                    .child(editor_area)
                    .children(self.create_error.as_ref().filter(|_| kind == "create").map(
                        |error| {
                            div()
                                .text_size(px(13.))
                                .text_color(rgb(0xf07b84))
                                .child(error.clone())
                        },
                    ))
                    .children(
                        (!single && document.markdown.contains("[ADF:"))
                            .then(|| muted("Leave [ADF:n] markers unchanged.")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                Button::new("save-edit")
                                    .primary()
                                    .small()
                                    .bg(rgb(0x3776c8))
                                    .text_color(rgb(0xffffff))
                                    .disabled(self.saving || self.picker.is_some())
                                    .loading(self.create_pending.is_some())
                                    .when(self.create_pending.is_some(), |button| {
                                        button.icon(gpui_kit::assets::IconName::Loader)
                                    })
                                    .label(if self.create_pending.is_some() {
                                        "Creating…"
                                    } else if self.saving {
                                        "Saving…"
                                    } else {
                                        "Save changes"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_edit(window, cx);
                                        cx.stop_propagation();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("cancel-edit")
                                    .small()
                                    .disabled(self.create_pending.is_some())
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.create_pending.is_some() {
                                            return;
                                        }
                                        this.edit = None;
                                        this.focus.focus(window, cx);
                                        cx.notify();
                                    })),
                            ),
                    );
        } else if let Some(picker) = &self.picker
            && picker.kind == "delete"
        {
            body = div()
                .flex()
                .flex_col()
                .flex_1()
                .p_6()
                .gap_3()
                .child(
                    div()
                        .text_size(px(22.))
                        .child(format!("Delete {}?", picker.key)),
                )
                .child(
                    Button::new("cancel-picker")
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, window, cx| this.close_picker(window, cx))),
                )
                .child(
                    Button::new("confirm-delete")
                        .label("Delete permanently")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.select_picker(window, cx)),
                        ),
                );
        }
        let shortcuts = self.render_shortcuts(cx);
        if self.create_refocus {
            self.create_refocus = false;
            if self.edit.is_some() {
                self.summary_input
                    .update(cx, |input, cx| input.focus(window, cx));
            } else {
                self.focus.focus(window, cx);
            }
        }
        let popup = self
            .picker
            .as_ref()
            .filter(|p| p.kind != "delete")
            .map(|_| self.render_picker(cx));
        if self.related_reveal {
            self.related_reveal = false;
            self.scroll_related_selection(window, cx);
        }
        div()
            .id("workspace")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .font_family("Inter")
            .text_size(px(13.))
            .bg(rgb(0x191c23))
            .text_color(rgb(0xe6ebf3))
            .track_focus(&self.focus)
            .key_context("Workspace")
            .on_action(cx.listener(Self::command))
            .capture_key_down(cx.listener(Self::picker_key))
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.related_navigation.is_some()
                        && this.edit.is_none()
                        && !this.palette
                        && this.picker.is_none()
                    {
                        this.related_input
                            .update(cx, |input, cx| input.focus(window, cx));
                    } else {
                        this.focus.focus(window, cx);
                    }
                }),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if this.picker.is_some() {
                    this.close_picker(window, cx);
                }
            }))
            .child(
                TitleBar::new().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(img("images/jira.png").size(px(16.)).flex_shrink_0())
                        .child("not-bad-jira"),
                ),
            )
            .child(body)
            .children(popup)
            .child(
                div()
                    .h(px(36.))
                    .px_6()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(0x303744))
                    .child(
                        muted(self.status.clone())
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .pr_4(),
                    )
                    .child(shortcuts),
            )
    }
}

fn key_name(key: &str) -> String {
    match key {
        "enter" => "Enter".into(),
        "escape" => "Esc".into(),
        "delete" => "Delete".into(),
        "ctrl-," => "Ctrl+,".into(),
        "ctrl-shift-c" => "Ctrl+Shift+C".into(),
        _ => key.to_uppercase(),
    }
}
fn command_row(id: impl Into<ElementId>, label: String, hint: String, selected: bool) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .h(px(32.))
        .rounded_none()
        .bg(rgb(if selected { 0x253348 } else { 0x20252e }))
        .border_l_2()
        .border_color(rgb(if selected { 0x79aaf9 } else { 0x20252e }))
        .accessibility_label(format!("{label} {hint}"))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(label)
                .child(keycap(hint)),
        )
}

fn keycap(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(20.))
        .min_w(px(20.))
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .font_family("JetBrains Mono")
        .text_size(px(11.))
        .line_height(px(16.))
        .rounded(px(3.))
        .border_1()
        .border_color(rgb(0x3b4351))
        .bg(rgb(0x20252e))
        .text_color(rgb(0xc3ccdb))
        .child(label.into())
}
impl AppView {
    fn shortcut_button(
        &self,
        id: &'static str,
        keys: &[&str],
        label: &'static str,
        command: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        let name = match command {
            "j" => "Move down",
            "k" => "Move up",
            "picker-up" => "Previous option",
            "picker-down" => "Next option",
            _ => label,
        };
        let content = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .children(keys.iter().map(|key| keycap((*key).to_string())));
        let content = if label.is_empty() {
            content
        } else {
            content.child(muted(label))
        };
        Button::new(id)
            .ghost()
            .small()
            .h(px(24.))
            .px_1p5()
            .cursor_pointer()
            .accessibility_label(format!("{} ({})", name, keys.join(" / ")))
            .child(content)
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if this.create_pending.is_some() {
                    return;
                }
                match command {
                    "related-up" | "related-down" => {
                        this.move_related(command == "related-down", window, cx)
                    }
                    "related-open" => this.open_related_selection(window, cx),
                    "related-cancel" => this.close_related_navigation(window, cx),
                    "picker-up" | "picker-down" => {
                        if let Some(picker) = this.picker.as_mut() {
                            picker.move_selection(command == "picker-down");
                            this.picker_scroll
                                .scroll_to_item(picker.selected, ScrollStrategy::Nearest);
                            this.picker_input
                                .update(cx, |input, cx| input.focus(window, cx));
                        }
                    }
                    "picker-select" => this.select_picker(window, cx),
                    "palette" => {
                        this.palette = true;
                        this.inputs[5].update(cx, |s, cx| {
                            s.set_value("", window, cx);
                            s.focus(window, cx);
                        });
                    }
                    "palette-enter" => this.palette_execute(window, cx),
                    "save-edit" => this.save_edit(window, cx),
                    "escape" => {
                        if this.edit.take().is_some()
                            || this.picker.take().is_some()
                            || this.palette
                        {
                            this.palette = false;
                            this.focus.focus(window, cx);
                        } else {
                            this.run_command("escape", window, cx);
                        }
                    }
                    _ => this.run_command(command, window, cx),
                }
                cx.notify();
            }))
    }
}
