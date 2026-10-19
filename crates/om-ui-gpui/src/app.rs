//! Application shell: opens the themed main window and lays out the decks, the mixer and the
//! library placeholder from `om-ui`'s main layout.

use gpui::{div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Window, WindowBounds, WindowOptions};

/// Window placeholder while the views are being ported.
struct Shell;

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x101418)).child("OpenMixer")
    }
}

/// Opens the main window and runs the GPUI event loop until it closes.
pub fn run() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Shell),
        )
        .expect("open main window");
        cx.activate(true);
    });
}
