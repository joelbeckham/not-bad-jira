use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, tooltip::Tooltip};
use gpui_kit::*;
use std::borrow::Cow;

gpui_kit::assets::icon_assets!(
    IndicatorAssets,
    [
        ChevronsUp,
        ChevronsDown,
        Equal,
        CircleDashed,
        CircleDot,
        CircleUserRound,
        Bug,
        Bookmark,
        SquareCheck,
        Zap,
        Copy,
        Diamond
    ]
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "images/jira.png" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/jira.png"))));
        }
        if let Some(asset) = IndicatorAssets.load(path)? {
            return Ok(Some(asset));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(IndicatorAssets.list(path)?);
        Ok(assets)
    }
}

pub fn icon(
    label: String,
    id: impl Into<ElementId>,
    glyph: IconName,
    color: u32,
) -> impl IntoElement {
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

pub fn labeled(label: String, glyph: IconName, color: u32) -> impl IntoElement {
    div()
        .h(px(20.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_color(rgb(color))
        .child(
            div()
                .size(px(16.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(glyph).size(px(16.))),
        )
        .child(label)
}
