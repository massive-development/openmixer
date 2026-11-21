//! Opens a plugin's own window (ADR 0007, "GUI strategy") from the Plugins page.
//!
//! X11: the plugin is embedded in a window of its own that holds nothing else, because CLAP gives
//! the host no way to place the plugin's child inside a sub-rectangle. Wayland: no window of ours
//! is opened; the plugin floats beside the app. Anything else: the page shows the parameter panel.
//! UNVERIFIED on a display: whether the webview's X11 window keeps the plugin's child visible, and
//! whether a Wayland floating window opens. The pure parts are unit-tested.

use om_ui::settings::{PluginSession, PluginWindow, PluginWindowMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{
    AppHandle, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

/// How often an embedded window asks the plugin whether it wants another size.
const RESIZE_POLL: Duration = Duration::from_millis(120);

thread_local! {
    /// Plugin ids whose embedding window is open, so a second press does not open another.
    static OPEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

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

fn native_of_window(window: &WebviewWindow) -> (PluginSession, Option<u64>) {
    window
        .window_handle()
        .map_or((PluginSession::Other, None), |h| native_of(&h.as_raw()))
}

/// Opens the plugin's window as its mode says. `from` is the window the request came from: its
/// kind decides the mode, and on X11 a floating plugin stays above it. Says what happened.
pub(crate) fn open(
    app: &AppHandle,
    from: &WebviewWindow,
    id: &str,
    source: Arc<dyn PluginWindow>,
) -> Result<String, String> {
    let (session, from_id) = native_of_window(from);
    let scale = from.scale_factor().unwrap_or(1.0);
    match source.mode(session) {
        PluginWindowMode::Parameters => {
            Err("This plugin's window cannot be shown here; use Parameters.".to_owned())
        }
        PluginWindowMode::Floating => source
            .open(PluginWindowMode::Floating, from_id, scale)
            .map(|_| format!("Opened the window of {id}"))
            .map_err(|e| e.to_string()),
        PluginWindowMode::Embedded => open_embedded(app, id, source, scale),
    }
}

/// A window label from a plugin id: Tauri labels allow letters, digits and `-/:_` only.
fn label_of(id: &str) -> String {
    let safe: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("plugin-{safe}")
}

fn open_embedded(
    app: &AppHandle,
    id: &str,
    source: Arc<dyn PluginWindow>,
    scale: f64,
) -> Result<String, String> {
    if OPEN.with(|open| open.borrow().iter().any(|p| p == id)) {
        return Err("The plugin window is already open.".to_owned());
    }
    let url = WebviewUrl::External("about:blank".parse().map_err(|e| format!("{e}"))?);
    let window = WebviewWindowBuilder::new(app, label_of(id), url)
        .title(id)
        .inner_size(480.0, 320.0)
        .build()
        .map_err(|e| e.to_string())?;
    let (_, parent) = native_of_window(&window);
    match source.open(PluginWindowMode::Embedded, parent, scale) {
        Ok(Some((w, h))) => {
            let _ = window.set_size(PhysicalSize::new(w, h));
        }
        Ok(None) => {}
        Err(error) => {
            let _ = window.destroy();
            return Err(error.to_string());
        }
    }
    OPEN.with(|open| open.borrow_mut().push(id.to_owned()));
    follow(&window, id, source);
    Ok(format!("Opened the window of {id}"))
}

/// Keeps the window as big as the plugin asks, and closes the plugin's window before ours goes.
fn follow(window: &WebviewWindow, id: &str, source: Arc<dyn PluginWindow>) {
    let alive = Arc::new(AtomicBool::new(true));
    let (poll_alive, poll_source, poll_window) =
        (Arc::clone(&alive), Arc::clone(&source), window.clone());
    let polled = std::thread::Builder::new()
        .name("plugin-window".to_owned())
        .spawn(move || {
            while poll_alive.load(Ordering::Acquire) {
                std::thread::sleep(RESIZE_POLL);
                if let Some((w, h)) = poll_source.take_resize_request() {
                    let _ = poll_window.set_size(PhysicalSize::new(w, h));
                    if let Err(error) = poll_source.resize(w, h) {
                        tracing::warn!(%error, "plugin window resize");
                    }
                }
            }
        });
    if let Err(error) = polled {
        tracing::warn!(%error, "plugin window resize thread");
    }
    let id = id.to_owned();
    window.on_window_event(move |event| {
        let ending = matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        );
        // Closed once: a close request is followed by the destroy.
        if ending && alive.swap(false, Ordering::AcqRel) {
            source.close();
            OPEN.with(|open| open.borrow_mut().retain(|p| p != &id));
        }
    });
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
    }

    #[test]
    fn a_label_keeps_only_what_tauri_allows() {
        assert_eq!(label_of("a.b/c d"), "plugin-a-b-c-d");
    }
}
