use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, tooltip::Tooltip};
use gpui_kit::*;
use serde_json::Value;
use std::sync::Arc;

pub fn name(kind: &Value) -> String {
    kind["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or("Issue")
        .into()
}

pub fn icon(kind: &Value, image: Option<Arc<RenderImage>>, id: impl Into<ElementId>) -> AnyElement {
    let label = name(kind);
    let (glyph, color) = if kind["subtask"] == true {
        (IconName::Copy, 0x8cc7df)
    } else {
        match label.to_ascii_lowercase().as_str() {
            "bug" => (IconName::Bug, 0xf07b84),
            "story" => (IconName::Bookmark, 0x83d4ac),
            "task" => (IconName::SquareCheck, 0x91b9f7),
            "epic" => (IconName::Zap, 0xc095f4),
            "sub-task" | "subtask" => (IconName::Copy, 0x8cc7df),
            _ => (IconName::Diamond, 0xa4afc0),
        }
    };
    let glyph_size = match glyph {
        IconName::Bug | IconName::Copy => 14.,
        IconName::Bookmark => 17.,
        _ => 16.,
    };
    let image = image
        .map(|image| img(image).size(px(16.)).into_any_element())
        .unwrap_or_else(|| {
            Icon::new(glyph)
                .size(px(glyph_size))
                .text_color(rgb(color))
                .into_any_element()
        });
    div()
        .id(id)
        .size(px(20.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .role(Role::Image)
        .aria_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        .child(image)
        .into_any_element()
}
