use gpui_kit::*;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Deserialize, Action)]
#[action(namespace = workspace, no_json)]
pub struct Command {
    pub key: String,
}
pub const COMMANDS: &[(&str, &str)] = &[
    ("enter", "Open issue"),
    ("escape", "Back"),
    ("r", "Refresh"),
    ("s", "Change status"),
    ("p", "Change priority"),
    ("a", "Change assignee"),
    ("t", "Edit title"),
    ("e", "Edit description"),
    ("m", "Add comment"),
    ("i", "Assign to me"),
    ("d", "Mark done"),
    ("delete", "Delete issue"),
    ("c", "Create issue"),
    ("y", "Copy issue key"),
    ("u", "Copy issue URL"),
    ("o", "Open in browser"),
    ("ctrl-,", "Open config file"),
    ("ctrl-shift-c", "Connect account"),
    ("q", "Quit"),
];
pub fn issue_commands() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        "enter", "s", "p", "a", "t", "e", "m", "i", "d", "y", "u", "o", "delete",
    ]
    .into_iter()
    .filter_map(|key| {
        COMMANDS
            .iter()
            .copied()
            .find(|(command, _)| *command == key)
    })
}

pub fn bind(cx: &mut App) {
    // Registered after zorite-editor's bindings so Escape cancels the host edit.
    cx.bind_keys([KeyBinding::new(
        "escape",
        Command {
            key: "cancel-edit".into(),
        },
        Some("Editor"),
    )]);
    for (i, key) in [
        "shift-1", "shift-2", "shift-3", "shift-4", "shift-5", "shift-6", "shift-7", "shift-8",
        "shift-9",
    ]
    .iter()
    .enumerate()
    {
        cx.bind_keys([KeyBinding::new(
            key,
            Command {
                key: format!("sort-{}", i + 1),
            },
            Some("Workspace"),
        )]);
    }
    for (key, _) in COMMANDS {
        cx.bind_keys([KeyBinding::new(
            key,
            Command { key: (*key).into() },
            Some("Workspace"),
        )]);
    }
    for key in [
        "j", "k", "up", "down", "home", "end", "h", "l", "left", "right", "1", "2", "3", "4", "5",
        "6", "7", "8", "9", "/",
    ] {
        cx.bind_keys([KeyBinding::new(
            key,
            Command { key: key.into() },
            Some("Workspace"),
        )]);
    }
}
