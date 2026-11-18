use super::*;
use crate::{config::Tab, tabs::Change};
use gpui_kit::base::Checkbox;
use gpui_kit::component::{Icon, input::Textarea, menu::PopupMenu};

pub(super) struct TabEditor {
    index: Option<usize>,
    draft: Tab,
    rename: bool,
    pub preview: Option<(Vec<Issue>, bool)>,
    pub preview_loading: bool,
    pub preview_error: Option<String>,
    error: Option<String>,
    scroll: UniformListScrollHandle,
}
pub(super) struct TabSave {
    change: Change,
    config: Config,
    notice: &'static str,
}

impl AppView {
    fn tabs_available(&self) -> bool {
        self.tab_save.is_none()
            && self.tab_editor.is_none()
            && self.edit.is_none()
            && self.picker.is_none()
            && !self.saving
            && !self.connecting
    }

    fn begin_tab_editor(
        &mut self,
        index: Option<usize>,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tabs_available() {
            return;
        }
        let tab = index
            .and_then(|i| self.config.as_ref()?.tabs.get(i).cloned())
            .unwrap_or_else(|| Tab::with_query(String::new(), String::new()));
        self.action_notice = None;
        self.palette = false;
        self.tab_name.update(cx, |input, cx| {
            input.set_value(tab.label.clone(), window, cx);
            input.focus(window, cx);
        });
        self.tab_jql
            .update(cx, |input, cx| input.set_value(tab.query(), window, cx));
        self.tab_preview_request += 1;
        self.tab_editor = Some(TabEditor {
            index,
            draft: tab,
            rename,
            preview: None,
            preview_loading: false,
            preview_error: None,
            error: None,
            scroll: UniformListScrollHandle::new(),
        });
        // Preview is explicit, so typing never starts overlapping network requests.
        cx.notify();
    }

    pub(super) fn close_tab_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab_save.is_some() {
            return;
        }
        self.tab_editor = None;
        self.tab_preview_request += 1;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn demo_tab_results(&self, query: &str) -> Result<Vec<Issue>, String> {
        let mut issues: Vec<_> = if let Some(key) = query
            .strip_prefix("parent = \"")
            .and_then(|s| s.split('"').next())
        {
            self.store
                .issues
                .values()
                .filter(|issue| issue.fields["parent"]["key"].as_str() == Some(key))
                .cloned()
                .collect()
        } else if query
            == "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"
        {
            self.store
                .tabs
                .first()
                .into_iter()
                .flatten()
                .filter_map(|key| self.store.issues.get(key).cloned())
                .collect()
        } else {
            return Err("Custom JQL preview requires a connected Jira account".into());
        };
        issues.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(issues)
    }

    fn preview_tab(&mut self, cx: &mut Context<Self>) {
        if self.tab_save.is_some() || self.tab_editor.as_ref().is_none_or(|e| e.preview_loading) {
            return;
        }
        let query = self.tab_jql.read(cx).value().to_string();
        self.tab_preview_request += 1;
        let request = self.tab_preview_request;
        let demo_result = self.demo.then(|| {
            self.demo_tab_results(query.trim()).map(|mut rows| {
                let more = rows.len() > 50;
                rows.truncate(50);
                (rows, more)
            })
        });
        let editor = self.tab_editor.as_mut().unwrap();
        editor.preview = None;
        editor.preview_error = None;
        editor.scroll.scroll_to_item(0, ScrollStrategy::Top);
        if query.trim().is_empty() {
            editor.preview_error = Some("Enter JQL".into());
        } else if let Some(result) = demo_result {
            match result {
                Ok(preview) => editor.preview = Some(preview),
                Err(error) => editor.preview_error = Some(error),
            }
        } else if let Some(client) = self.client.clone() {
            editor.preview_loading = true;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Message::TabPreview(
                    request,
                    client.preview_jql(&query).map_err(|e| e.to_string()),
                ));
            });
        } else {
            editor.preview_error = Some("Connect to Jira to preview this query".into());
        }
        cx.notify();
    }

    pub(super) fn save_tab_editor(&mut self, cx: &mut Context<Self>) {
        if self.tab_save.is_some() {
            return;
        }
        let Some(editor) = self.tab_editor.as_mut() else {
            return;
        };
        let mut tab = editor.draft.clone();
        tab.label = self.tab_name.read(cx).value().trim().to_string();
        if tab.label.is_empty() {
            editor.error = Some("Enter a tab name".into());
            cx.notify();
            return;
        }
        if !editor.rename {
            let query = self.tab_jql.read(cx).value().trim().to_string();
            if query.is_empty() {
                editor.error = Some("Enter JQL".into());
                cx.notify();
                return;
            }
            if query != tab.query() {
                tab.jql = query;
                tab.filter_id.clear();
            }
        }
        let change = editor
            .index
            .map(|i| Change::Replace(i, tab.clone()))
            .unwrap_or(Change::Add(tab, None));
        self.persist_tab_change(change, "Tab saved", cx);
    }

    fn persist_tab_change(&mut self, change: Change, notice: &'static str, cx: &mut Context<Self>) {
        if self.tab_save.is_some() || self.saving {
            return;
        }
        let Some(old) = self.config.clone() else {
            return;
        };
        let next = match change.config(&old) {
            Ok(c) => c,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        self.tab_save = Some(TabSave {
            change,
            config: next.clone(),
            notice,
        });
        self.action_notice = None;
        if let Some(editor) = self.tab_editor.as_mut() {
            editor.error = None;
        }
        self.status = "Saving tab…".into();
        if self.demo {
            self.finish_tab_save(Ok(()), cx);
        } else {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = next.save_tabs(&old).map_err(|e| e.to_string());
                let _ = tx.send(Message::TabsSaved(result));
            });
        }
        cx.notify();
    }

    pub(super) fn finish_tab_save(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        let Some(save) = self.tab_save.take() else {
            return;
        };
        if let Err(error) = result {
            self.status = format!("Tab save failed: {error}");
            if let Some(editor) = self.tab_editor.as_mut() {
                editor.error = Some(error);
            }
            cx.notify();
            return;
        }
        let old = self.config.as_ref().unwrap();
        self.tab = save.change.apply(old, &mut self.store, self.tab);
        self.sort = self
            .sort
            .drain()
            .filter_map(|(index, sort)| match &save.change {
                Change::Delete(deleted) if index == *deleted => None,
                Change::Delete(deleted) if index > *deleted => Some((index - 1, sort)),
                Change::Replace(changed, _) if index == *changed => None,
                _ => Some((index, sort)),
            })
            .collect();
        self.revision += 1;
        self.tab_generation += 1;
        self.tab_preview_request += 1;
        self.tab_editor = None;
        self.tab_refocus = true;
        if self.pending_config.as_ref() == Some(&save.config) {
            self.pending_config = None;
        }
        self.config = Some(save.config.clone());
        if let Some(client) = self.client.as_mut() {
            client.config = save.config.clone();
        }
        self.selected = 0;
        self.stack.clear();
        self.related_navigation = None;
        self.related_pending.clear();
        self.related_views.borrow_mut().clear();
        self.detail_offsets.clear();
        self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        if self.demo {
            let query = save.config.tabs.get(self.tab).map(Tab::query);
            if let Some(query) = query
                && let Ok(rows) = self.demo_tab_results(&query)
            {
                self.store.set_tab(self.tab, rows);
            }
            self.status = "Demo workspace · sample data".into();
        } else {
            let _ = self.cache_writer.send((self.store.clone(), save.config));
            self.status = "Tab saved".into();
            self.refresh();
        }
        self.show_notice(save.notice, true);
        cx.notify();
    }

    fn tab_action(
        &mut self,
        index: usize,
        generation: u64,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.tab_generation || !self.tabs_available() {
            return;
        }
        let Some(mut tab) = self
            .config
            .as_ref()
            .and_then(|c| c.tabs.get(index))
            .cloned()
        else {
            return;
        };
        match action {
            "Edit" | "Rename" => self.begin_tab_editor(Some(index), action == "Rename", window, cx),
            "Delete" => self.persist_tab_change(Change::Delete(index), "Tab deleted", cx),
            "Duplicate" => {
                let base = format!("{} copy", tab.label);
                tab.label = base.clone();
                let mut number = 2;
                while self
                    .config
                    .as_ref()
                    .unwrap()
                    .tabs
                    .iter()
                    .any(|t| t.label == tab.label)
                {
                    tab.label = format!("{base} {number}");
                    number += 1;
                }
                self.persist_tab_change(Change::Add(tab, Some(index)), "Tab duplicated", cx);
            }
            _ => {}
        }
    }

    pub(super) fn epic_menu(menu: PopupMenu, weak: WeakEntity<Self>, issue: Issue) -> PopupMenu {
        if !crate::tabs::is_epic(&issue) {
            return menu;
        }
        menu.item(
            PopupMenuItem::new("Turn into tab").on_click(move |_, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if !this.tabs_available() {
                        return;
                    }
                    if let Ok(tab) = Tab::for_epic(&issue) {
                        this.persist_tab_change(Change::Add(tab, None), "Epic tab created", cx);
                    }
                });
            }),
        )
    }

    pub(super) fn render_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = self.config.as_ref().unwrap();
        let generation = self.tab_generation;
        if self.tab_strip_selection.replace((self.tab, generation)) != (self.tab, generation) {
            self.tab_strip_scroll.scroll_to_item(self.tab);
        }
        let enabled = self.tabs_available();
        let weak = cx.entity().downgrade();
        div()
            .flex()
            .h(px(40.))
            .flex_shrink_0()
            .px_6()
            .border_b_1()
            .border_color(rgb(0x2b303b))
            .child(
                div()
                    .id("tab-strip")
                    .track_scroll(&self.tab_strip_scroll)
                    .flex()
                    .min_w_0()
                    .max_w(relative(0.9))
                    .overflow_x_scroll()
                    .children(c.tabs.iter().enumerate().map(|(i, t)| {
                        let weak = cx.entity().downgrade();

                        div()
                            .id(("tab", i))
                            .h_full()
                            .max_w(px(220.))
                            .flex_shrink_0()
                            .mr_6()
                            .flex()
                            .items_center()
                            .border_b_2()
                            .border_color(rgb(if i == self.tab { 0x79aaf9 } else { 0x191c23 }))
                            .text_color(rgb(if i == self.tab { 0xf0f3f9 } else { 0x929cad }))
                            .cursor_pointer()
                            .hover(|tab| tab.bg(rgb(0x202832)))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(t.label.clone()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !this.tabs_available() {
                                    return;
                                }
                                this.tab = i;
                                this.selected = 0;
                                this.stack.clear();
                                this.related_navigation = None;
                                this.related_views.borrow_mut().clear();
                                this.related_pending.clear();
                                this.detail_offsets.clear();
                                this.focus.focus(window, cx);
                                cx.notify();
                            }))
                            .context_menu(move |mut menu, _, _| {
                                for action in ["Edit", "Rename", "Duplicate", "Delete"] {
                                    if action == "Delete" {
                                        menu = menu.separator();
                                    }
                                    let weak = weak.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(format!("{action} tab"))
                                            .disabled(!enabled)
                                            .on_click(move |_, window, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.tab_action(
                                                        i, generation, action, window, cx,
                                                    )
                                                });
                                            }),
                                    );
                                }
                                menu
                            })
                    })),
            )
            .child(
                div()
                    .id("tab-empty-space")
                    .flex_1()
                    .h_full()
                    .min_w(px(40.))
                    .context_menu(move |menu, _, _| {
                        let weak = weak.clone();
                        menu.item(PopupMenuItem::new("New tab").disabled(!enabled).on_click(
                            move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.begin_tab_editor(None, false, window, cx)
                                });
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_tab_editor(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.tab_editor.as_ref().unwrap();
        let pending = self.tab_save.is_some();
        let title = if editor.rename {
            "Rename tab"
        } else if editor.index.is_some() {
            "Edit tab"
        } else {
            "New tab"
        };
        let heading = if editor.index.is_some() {
            format!("{} / {title}", editor.draft.label)
        } else {
            title.into()
        };
        let name_focus = self.tab_name.read(cx).focus_handle(cx).is_focused(window);
        let jql_focus = self.tab_jql.read(cx).focus_handle(cx).is_focused(window);
        let name = div().flex().flex_col().gap_2().child(muted("Name")).child(
            Input::new(&self.tab_name)
                .aria_label("Tab name")
                .focus_bordered(false)
                .border_color(rgb(if name_focus { 0x79aaf9 } else { 0x394352 }))
                .disabled(pending),
        );
        let mut form = div()
            .id("tab-editor")
            .capture_action(cx.listener(
                |this, action: &gpui_kit::component::input::Enter, _, cx| {
                    if action.secondary {
                        this.save_tab_editor(cx);
                        cx.stop_propagation();
                    } else {
                        cx.propagate();
                    }
                },
            ))
            .on_action(
                cx.listener(|this, _: &gpui_kit::component::input::Escape, window, cx| {
                    this.close_tab_editor(window, cx);
                }),
            )
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .p_6()
            .gap_3()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(muted(heading))
            .child(name);
        if !editor.rename {
            let jql = div()
                .border_1()
                .rounded_md()
                .overflow_hidden()
                .border_color(rgb(if jql_focus { 0x79aaf9 } else { 0x394352 }))
                .child(
                    Textarea::new(&self.tab_jql)
                        .aria_label("JQL")
                        .bordered(false)
                        .h(px(80.))
                        .font_family("JetBrains Mono")
                        .text_size(px(12.))
                        .disabled(pending),
                );
            form = form
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(muted("JQL"))
                        .child(jql),
                )
                .child(self.tab_columns(editor, pending, cx))
                .child(self.tab_preview_header(cx))
                .child(self.tab_preview_results(cx));
        }
        if let Some(error) = &editor.error {
            form = form.child(div().text_color(rgb(0xf0a5a5)).child(error.clone()));
        }
        form.child(self.tab_editor_actions(cx)).into_any_element()
    }

    fn tab_columns(&self, editor: &TabEditor, pending: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_shrink_0()
            .child(muted("Columns"))
            .child(
                div().flex().flex_wrap().gap_x_4().gap_y_2().children(
                    crate::config::LIST_COLUMNS
                        .into_iter()
                        .map(|(field, label)| {
                            let columns = &editor.draft.columns;
                            let checked = columns.iter().any(|column| column == field);
                            let disabled = pending
                                || (checked && columns.iter().all(|column| column == field));
                            let toggle = cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(editor) = this.tab_editor.as_mut() {
                                    if checked {
                                        editor.draft.columns.retain(|column| column != field);
                                    } else {
                                        editor.draft.columns.push(field.into());
                                    }
                                    cx.notify();
                                }
                            });
                            Checkbox::new(SharedString::from(format!("tab-column-{field}")))
                                .accessibility_label(label)
                                .checked(checked)
                                .disabled(disabled)
                                .flex()
                                .items_center()
                                .gap_2()
                                .rounded(px(3.))
                                .border_1()
                                .border_color(gpui_kit::transparent_black())
                                .focus_visible(|style| style.border_color(rgb(0x79aaf9)))
                                .text_size(px(13.))
                                .when(!disabled, |checkbox| checkbox.cursor_pointer())
                                .when(disabled, |checkbox| checkbox.opacity(0.55))
                                .child(
                                    div()
                                        .size(px(14.))
                                        .rounded(px(3.))
                                        .border_1()
                                        .border_color(rgb(if checked {
                                            0x79aaf9
                                        } else {
                                            0x697688
                                        }))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(checked, |indicator| {
                                            indicator.bg(rgb(0x79aaf9)).child(
                                                Icon::new(gpui_kit::assets::IconName::Check)
                                                    .size(px(12.))
                                                    .text_color(rgb(0x17212e)),
                                            )
                                        }),
                                )
                                .child(label)
                                .on_change(move |_, event, window, cx| toggle(event, window, cx))
                        }),
                ),
            )
    }

    fn tab_editor_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let pending = self.tab_save.is_some();
        div()
            .flex()
            .gap_2()
            .flex_shrink_0()
            .child(
                Button::new("save-tab")
                    .small()
                    .primary()
                    .bg(rgb(0x3776c8))
                    .text_color(rgb(0xffffff))
                    .disabled(pending)
                    .label(if pending { "Saving?" } else { "Save" })
                    .on_click(cx.listener(|this, _, _, cx| this.save_tab_editor(cx))),
            )
            .child(
                Button::new("cancel-tab")
                    .small()
                    .disabled(pending)
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.close_tab_editor(window, cx))),
            )
            .into_any_element()
    }

    fn tab_preview_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.tab_editor.as_ref().unwrap();
        let count = editor
            .preview
            .as_ref()
            .map(|(rows, more)| format!("{}{} issues", rows.len(), if *more { "+" } else { "" }))
            .unwrap_or_else(|| {
                if editor.preview_loading {
                    "Loading?"
                } else {
                    "Not run"
                }
                .into()
            });
        div()
            .flex()
            .items_center()
            .gap_3()
            .flex_shrink_0()
            .pt_2()
            .child(div().flex_1().child(muted("Preview")))
            .child(muted(count))
            .child(
                Button::new("preview-tab")
                    .small()
                    .disabled(self.tab_save.is_some() || editor.preview_loading)
                    .label("Run preview")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.preview_tab(cx);
                        this.tab_jql.update(cx, |input, cx| input.focus(window, cx));
                    })),
            )
            .into_any_element()
    }

    fn tab_preview_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.tab_editor.as_ref().unwrap();
        let mut panel = div()
            .relative()
            .flex_1()
            .min_h_0()
            .border_1()
            .border_color(rgb(0x303744))
            .rounded_md()
            .overflow_hidden();
        if let Some(error) = &editor.preview_error {
            panel = panel.child(div().p_3().text_color(rgb(0xf0a5a5)).child(error.clone()));
        } else if let Some((rows, _)) = &editor.preview {
            if rows.is_empty() {
                panel = panel.child(div().p_3().child(muted("No matching issues")));
            } else {
                let rows = rows.clone();
                let scroll = editor.scroll.clone();
                panel = panel
                    .child(
                        uniform_list(
                            "tab-preview",
                            rows.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                                range
                                    .map(|i| {
                                        let issue = &rows[i];
                                        div()
                                            .w_full()
                                            .h(px(32.))
                                            .pl_3()
                                            .pr(px(24.))
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .w(px(150.))
                                                    .flex_shrink_0()
                                                    .child(this.issue_key(issue, true, window, cx)),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .child(issue.field("summary")),
                                            )
                                            .child(
                                                div()
                                                    .w(px(32.))
                                                    .flex_shrink_0()
                                                    .flex()
                                                    .justify_center()
                                                    .child(crate::status::icon(
                                                        &issue.fields["status"],
                                                        ("preview-status", i),
                                                    )),
                                            )
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(&scroll)
                        .size_full(),
                    )
                    .child(Scrollbar::vertical(&scroll).mode(ScrollbarMode::Always));
            }
        }
        panel.into_any_element()
    }

    pub(super) fn new_tab_button(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(Button::new("new-tab").label("New tab").on_click(
                cx.listener(|this, _, window, cx| this.begin_tab_editor(None, false, window, cx)),
            ))
            .into_any_element()
    }
}
