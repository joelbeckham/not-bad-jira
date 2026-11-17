use gpui_kit::assets::IconName;
use gpui_kit::*;
use serde_json::Value;

fn appearance(status: &Value) -> (String, IconName, u32) {
    let name = status["name"].as_str().unwrap_or("");
    // Waiting has a distinct meaning even within Jira's in-progress category.
    if name.trim().eq_ignore_ascii_case("waiting") {
        return (name.to_string(), IconName::Clock, 0xe8bd70);
    }
    // The category handles other custom workflow names without guessing their meaning.
    let category = status["statusCategory"]["key"].as_str().unwrap_or("");
    let category = if category.is_empty() {
        match name.to_ascii_lowercase().as_str() {
            "to do" | "open" | "backlog" | "reopened" => "new",
            "in progress" | "in review" | "review" | "testing" => "indeterminate",
            "done" | "closed" | "resolved" => "done",
            _ => "",
        }
    } else {
        category
    };
    let (glyph, color) = match category {
        "new" => (IconName::CircleDashed, 0xa4afc0),
        "indeterminate" => (IconName::CircleDot, 0x91b9f7),
        "done" => (IconName::CircleCheck, 0x83d4ac),
        _ => (IconName::CircleAlert, 0x929cad),
    };
    let label = if name.is_empty() { "No status" } else { name };
    (label.to_string(), glyph, color)
}

pub fn icon(status: &Value, id: impl Into<ElementId>) -> impl IntoElement {
    let (label, glyph, color) = appearance(status);
    crate::indicator::icon(label, id, glyph, color)
}

pub fn detail(status: &Value) -> impl IntoElement {
    let (label, glyph, color) = appearance(status);
    crate::indicator::labeled(label, glyph, color)
}
