#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use gpui_kit::component::{Theme, ThemeMode, TitleBar};
use gpui_kit::*;
mod actions;
mod adf;
mod app;
mod avatar;
mod config;
mod indicator;
mod issue_type;
mod jira;
mod picker;
mod priority;
mod related;
mod status;
mod store;
mod wheel;
fn main() {
    let started = std::time::Instant::now();
    let measure = std::env::args().any(|a| a == "--measure-startup");
    gpui_kit::application()
        .with_assets(indicator::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.text_system()
                .add_fonts(vec![
                    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter.ttf")),
                    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono.ttf")),
                ])
                .expect("embedded fonts");
            Theme::change(ThemeMode::Dark, None, cx);
            zorite_editor::bind_keys(cx);
            actions::bind(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_min_size: Some(size(px(800.), px(640.))),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1120.), px(760.)),
                        cx,
                    ))),
                    ..TitleBar::window_options()
                },
                cx,
                move |window, cx| {
                    if measure {
                        record_startup(window, started);
                    }
                    cx.new(|cx| app::AppView::new(window, cx))
                },
            )
            .expect("open window");
            cx.activate(true);
        });
}

fn record_startup(window: &mut Window, started: std::time::Instant) {
    window.on_next_frame(move|window,_|{
        window.on_next_frame(move|_,_|{
            let elapsed=started.elapsed();
            std::thread::spawn(move||{
                let _=std::fs::write("design/reviews/startup.txt",format!("Rust main to second frame callback (upper bound on first frame): {:.2} ms\n",elapsed.as_secs_f64()*1000.));
            });
        });
    });
}
