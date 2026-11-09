//! Every control part in every state, for checking the faceplate controls by eye:
//! `cargo run --locked -p om-ui-gpui --example controls_gallery` opens one window per built-in
//! theme and density (six). Name a theme and a density to open one:
//! `cargo run --locked -p om-ui-gpui --example controls_gallery -- daylight compact`.

use gpui::{point, prelude::*, px, size, App, Bounds, WindowBounds, WindowOptions};
use om_ui::theme::{Density, ThemeTokens};
use om_ui_gpui::gallery::{builtin_themes, ControlsGallery};

/// The (theme, density) pairs the arguments ask for: all of them when none are given.
fn wanted(args: &[String]) -> Vec<(String, ThemeTokens, Density)> {
    let density = |d: &Density| {
        let name = format!("{d:?}").to_lowercase();
        args.len() < 2 || args[1].to_lowercase() == name
    };
    let mut out = Vec::new();
    for (i, (name, tokens)) in builtin_themes().into_iter().enumerate() {
        let id = ["dark", "daylight", "harbour"]
            .get(i)
            .copied()
            .unwrap_or("");
        let theme_ok = args.is_empty() || args[0] == id || args[0] == name.to_lowercase();
        for d in [Density::Roomy, Density::Compact]
            .iter()
            .filter(|d| density(d))
        {
            if theme_ok {
                out.push((name.clone(), tokens.clone(), *d));
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    gpui_kit::application().run(move |cx: &mut App| {
        om_ui_gpui::font::register(cx);
        for (i, (name, tokens, density)) in wanted(&args).into_iter().enumerate() {
            let (w, h) = ControlsGallery::size(density);
            let offset = 40.0 * i as f32;
            let bounds = Bounds {
                origin: point(px(40.0 + offset), px(40.0 + offset)),
                size: size(px(w), px(h)),
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            cx.open_window(options, move |_, cx| {
                cx.new(|_| ControlsGallery::new(name, tokens, density))
            })
            .expect("open window");
        }
        cx.activate(true);
    });
}
