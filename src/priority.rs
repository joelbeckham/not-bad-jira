use gpui_kit::assets::IconName;
use gpui_kit::*;
fn appearance(name: String) -> (String, IconName, u32) {
    let (glyph, color) = match name.to_ascii_lowercase().as_str() {
        "highest" | "critical" | "blocker" => (IconName::ChevronsUp, 0xf07b84),
        "high" | "major" => (IconName::ChevronUp, 0xf29c58),
        "medium" | "normal" => (IconName::Equal, 0xa4afc0),
        "low" | "minor" => (IconName::ChevronDown, 0x9aabc5),
        "lowest" | "trivial" => (IconName::ChevronsDown, 0x8fa8c4),
        "" | "—" | "none" | "unprioritized" => (IconName::Minus, 0x929cad),
        _ => (IconName::CircleAlert, 0x929cad),
    };
    let label = if name.is_empty() || name == "—" {
        "No priority".to_string()
    } else {
        name
    };
    (label, glyph, color)
}

pub fn icon(name: String, id: impl Into<ElementId>) -> impl IntoElement {
    let (label, glyph, color) = appearance(name);
    crate::indicator::icon(label, id, glyph, color)
}

pub fn detail(name: String) -> impl IntoElement {
    let (label, glyph, color) = appearance(name);
    crate::indicator::labeled(label, glyph, color)
}
