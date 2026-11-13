use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, tooltip::Tooltip};
use gpui_kit::*;
use std::borrow::Cow;

gpui_kit::assets::icon_assets!(PriorityAssets, [ChevronsUp, ChevronsDown, Equal]);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(asset) = PriorityAssets.load(path)? {
            return Ok(Some(asset));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(PriorityAssets.list(path)?);
        Ok(assets)
    }
}

pub fn icon(name: String, id: impl Into<ElementId>) -> impl IntoElement {
    let (glyph, color) = match name.to_ascii_lowercase().as_str() {
        "highest" | "critical" | "blocker" => (IconName::ChevronsUp, 0xf07b84),
        "high" | "major" => (IconName::ChevronUp, 0xf29c58),
        "medium" | "normal" => (IconName::Equal, 0xa4afc0),
        "low" | "minor" => (IconName::ChevronDown, 0x77b9b0),
        "lowest" | "trivial" => (IconName::ChevronsDown, 0x8fa8c4),
        "" | "—" | "none" | "unprioritized" => (IconName::Minus, 0x929cad),
        _ => (IconName::CircleAlert, 0x929cad),
    };
    let label = if name.is_empty() || name == "—" {
        "No priority".to_string()
    } else {
        name
    };
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
        .child(Icon::new(glyph).size(px(16.)).text_color(rgb(color)))
}

pub fn detail_icon(name: String, id: impl Into<ElementId>) -> impl IntoElement {
    // Align the SVG's painted edge with the text label, accounting for its viewbox inset.
    div()
        .h(px(20.))
        .relative()
        .left(px(-5.))
        .top(px(1.))
        .child(icon(name, id))
}
