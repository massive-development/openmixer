//! A GPUI window that hosts a CLAP plugin's own window (ADR 0007, "GUI strategy").
//!
//! UNVERIFIED on a display: whether GPUI's X11 window keeps a foreign child window visible (it may
//! paint over it), where `close` runs relative to the platform window teardown, and whether a
//! Wayland floating window opens. This code compiles and its pure parts are unit-tested.
//!
//! X11: the plugin is embedded in a window of its own that holds nothing else, because CLAP gives the
//! host no way to place the plugin's child inside a sub-rectangle. Wayland: no window of ours is
//! opened; the plugin floats beside the app. Anything else: the caller shows the parameter panel.

use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};
use om_ui::settings::{PluginSession, PluginWindow, PluginWindowMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// Plugin ids whose embedding window is open, so a second press does not open another.
pub type OpenWindows = Rc<RefCell<Vec<String>>>;

/// How often the window asks the plugin whether it wants another size.
const RESIZE_POLL: Duration = Duration::from_millis(120);

/// The kind of window system `handle` belongs to, and the X11 window id when it is an X11 window.
// `c_ulong` (the Xlib window type) is `u32` on 32-bit targets, so the conversion is not always the identity.
#[allow(clippy::useless_conversion)]
pub(crate) fn native_of(handle: &RawWindowHandle) -> (PluginSession, Option<u64>) {
    match handle {
        RawWindowHandle::Xcb(h) => (PluginSession::X11, Some(u64::from(h.window.get()))),
        RawWindowHandle::Xlib(h) => (PluginSession::X11, Some(u64::from(h.window))),
        RawWindowHandle::Wayland(_) => (PluginSession::Wayland, None),
        _ => (PluginSession::Other, None),
    }
}

fn native_of_window(window: &Window) -> (PluginSession, Option<u64>) {
    HasWindowHandle::window_handle(window)
        .map_or((PluginSession::Other, None), |h| native_of(&h.as_raw()))
}

/// Opens the plugin's window as its mode says. `from` is the window the request came from: its kind
/// decides the mode, and on X11 a floating plugin stays above it.
pub(crate) fn open_plugin_window(
    cx: &mut App,
    from: &Window,
    id: &str,
    source: Arc<dyn PluginWindow>,
    open: &OpenWindows,
) -> Result<(), String> {
    let (session, from_id) = native_of_window(from);
    let scale = f64::from(from.scale_factor());
    match source.mode(session) {
        PluginWindowMode::Parameters => {
            Err("This plugin's window cannot be shown here; use Parameters.".to_owned())
        }
        PluginWindowMode::Floating => source
            .open(PluginWindowMode::Floating, from_id, scale)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        PluginWindowMode::Embedded => open_embedded(cx, id, source, open),
    }
}

fn open_embedded(
    cx: &mut App,
    id: &str,
    source: Arc<dyn PluginWindow>,
    open: &OpenWindows,
) -> Result<(), String> {
    if open.borrow().iter().any(|p| p == id) {
        return Err("The plugin window is already open.".to_owned());
    }
    let (plugin_id, open_list) = (id.to_owned(), Rc::clone(open));
    let bounds = Bounds::centered(None, size(px(480.), px(320.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from(id.to_owned())),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.open_window(options, move |window, cx| {
        cx.new(|cx| PluginWindowView::new(window, cx, plugin_id, source, open_list))
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// The window's content: nothing but the plugin's child window (or the reason it did not open).
struct PluginWindowView {
    id: String,
    source: Arc<dyn PluginWindow>,
    open: OpenWindows,
    error: Option<String>,
}

impl PluginWindowView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        id: String,
        source: Arc<dyn PluginWindow>,
        open: OpenWindows,
    ) -> Self {
        let (_, parent) = native_of_window(window);
        let scale = f64::from(window.scale_factor());
        let mut error = None;
        match source.open(PluginWindowMode::Embedded, parent, scale) {
            Ok(Some((w, h))) => resize_to(window, w, h, scale),
            Ok(None) => {}
            Err(e) => error = Some(e.to_string()),
        }
        open.borrow_mut().push(id.clone());
        // The plugin's window must be destroyed before ours (the contract of `PluginWindow::open`).
        let closing = Arc::clone(&source);
        window.on_window_should_close(cx, move |_, _| {
            closing.close();
            true
        });
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(RESIZE_POLL).await;
            let alive = this.update_in(cx, |view, window, _| view.follow_plugin(window));
            if alive.is_err() {
                break;
            }
        })
        .detach();
        Self {
            id,
            source,
            open,
            error,
        }
    }

    /// Applies a size the plugin asked for: resize our window, then tell the plugin it is done.
    fn follow_plugin(&mut self, window: &mut Window) {
        let Some((w, h)) = self.source.take_resize_request() else {
            return;
        };
        let scale = f64::from(window.scale_factor());
        resize_to(window, w, h, scale);
        if let Err(e) = self.source.resize(w, h) {
            self.error = Some(e.to_string());
        }
    }
}

/// Sets the window to the plugin's size. CLAP sizes on X11 are pixels; GPUI wants logical pixels.
fn resize_to(window: &mut Window, width: u32, height: u32, scale: f64) {
    let scale = scale.max(1.0);
    let logical = |v: u32| px((f64::from(v) / scale) as f32);
    window.resize(size(logical(width), logical(height)));
}

impl Drop for PluginWindowView {
    fn drop(&mut self) {
        self.source.close();
        self.open.borrow_mut().retain(|p| p != &self.id);
    }
}

impl Render for PluginWindowView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let text = self.error.clone().unwrap_or_default();
        div().size_full().child(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_window_handle::{WaylandWindowHandle, XcbWindowHandle};
    use std::num::NonZeroU32;
    use std::ptr::NonNull;

    #[test]
    fn x11_handles_give_the_window_id_and_wayland_gives_none() {
        let xcb = RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(4_194_311).unwrap()));
        assert_eq!(native_of(&xcb), (PluginSession::X11, Some(4_194_311)));
        let wl =
            RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::<u8>::dangling().cast()));
        assert_eq!(native_of(&wl), (PluginSession::Wayland, None));
        assert_eq!(
            native_of(&RawWindowHandle::Web(
                raw_window_handle::WebWindowHandle::new(1)
            ))
            .0,
            PluginSession::Other
        );
    }
}
