//! Showing a plugin's own window (ADR 0007, "GUI strategy").
//!
//! Plans, in order: embed into a window of ours (X11), float beside ours (Wayland, or X11 when the
//! plugin cannot embed), or fall back to the generic parameter panel. Every plugin call runs on the
//! plugin's control thread; the app only passes plain data ([`ParentWindow`] is an X11 window id).
//!
//! Unverified on real hardware: that a plugin's X11 child window stays visible inside a window drawn
//! by GPUI, and that a Wayland floating window opens. Only the plan logic and the call sequence are
//! tested (against a fixture whose GUI is a stub).

use super::worker::{Ctx, WorkerHandle};
use crate::PluginError;
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiError, GuiSize, PluginGui as GuiExt, Window,
};
use clack_host::prelude::PluginMainThreadHandle;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::Ordering;

/// What kind of windows the app shows, from the raw handle of one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiSession {
    X11,
    Wayland,
    /// Win32, Cocoa or anything else: the first release has no plugin windows there.
    Other,
}

/// A window API a plugin speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiApi {
    X11,
    Wayland,
}

/// How a plugin's GUI will be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiPlan {
    /// The plugin's window is a child of one of ours.
    Embedded(GuiApi),
    /// The plugin opens its own top-level window.
    Floating(GuiApi),
    /// The plugin's window cannot be shown here: use the generic parameter panel.
    Parameters,
}

/// The window a plugin is embedded into or floats above, as plain data that can cross threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentWindow {
    /// An X11 window id (what GPUI's X11 window reports through an XCB handle).
    X11(u64),
}

impl GuiSession {
    pub fn of(handle: &RawWindowHandle) -> Self {
        match handle {
            RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_) => GuiSession::X11,
            RawWindowHandle::Wayland(_) => GuiSession::Wayland,
            _ => GuiSession::Other,
        }
    }

    /// The session of `window`; `Other` when it has no handle yet.
    pub fn of_window<W: HasWindowHandle>(window: &W) -> Self {
        window
            .window_handle()
            .map_or(GuiSession::Other, |h| Self::of(&h.as_raw()))
    }
}

impl ParentWindow {
    /// The parent for an X11 window handle; `None` for any other kind (Wayland has no parent).
    pub fn of(handle: &RawWindowHandle) -> Option<Self> {
        match handle {
            RawWindowHandle::Xlib(h) => Some(ParentWindow::X11(u64::from(h.window))),
            RawWindowHandle::Xcb(h) => Some(ParentWindow::X11(u64::from(h.window.get()))),
            _ => None,
        }
    }

    pub fn of_window<W: HasWindowHandle>(window: &W) -> Option<Self> {
        Self::of(&window.window_handle().ok()?.as_raw())
    }

    fn to_clap(self) -> Window<'static, 'static> {
        match self {
            ParentWindow::X11(id) => Window::from_x11_handle(id as std::ffi::c_ulong),
        }
    }
}

impl GuiApi {
    fn clap_type(self) -> GuiApiType<'static> {
        match self {
            GuiApi::X11 => GuiApiType::X11,
            GuiApi::Wayland => GuiApiType::WAYLAND,
        }
    }

    fn of(api: GuiApiType<'_>) -> Option<Self> {
        [GuiApi::X11, GuiApi::Wayland]
            .into_iter()
            .find(|a| a.clap_type() == api)
    }
}

/// Picks how to show a plugin's GUI. `preferred` is what the plugin asked for, `supports` answers
/// `is_api_supported`. X11-only plugins on Wayland get the parameter panel: no XWayland (decision 5).
pub fn choose_plan(
    session: GuiSession,
    preferred: Option<(GuiApi, bool)>,
    supports: impl Fn(GuiApi, bool) -> bool,
) -> GuiPlan {
    let mut options: Vec<(GuiApi, bool)> = match session {
        GuiSession::X11 => vec![(GuiApi::X11, false), (GuiApi::X11, true)],
        GuiSession::Wayland => vec![(GuiApi::Wayland, true)],
        GuiSession::Other => Vec::new(),
    };
    if let Some(i) = preferred.and_then(|p| options.iter().position(|o| *o == p)) {
        options.swap(0, i);
    }
    match options
        .into_iter()
        .find(|&(api, floating)| supports(api, floating))
    {
        Some((api, false)) => GuiPlan::Embedded(api),
        Some((api, true)) => GuiPlan::Floating(api),
        None => GuiPlan::Parameters,
    }
}

/// The open plugin window, kept on the control thread.
pub(super) struct ActiveGui {
    ext: GuiExt,
    floating: bool,
}

/// A handle to one plugin instance's window. Cheap to clone; every call blocks on the plugin's
/// control thread, so call from the UI thread only for short operations.
#[derive(Clone)]
pub struct PluginGui {
    worker: WorkerHandle,
}

impl std::fmt::Debug for PluginGui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PluginGui")
    }
}

impl PluginGui {
    pub(super) fn new(worker: WorkerHandle) -> Self {
        Self { worker }
    }

    fn call<R: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Ctx) -> R + Send + 'static,
    ) -> Result<R, PluginError> {
        self.worker
            .call(job)
            .ok_or_else(|| PluginError::Rejected("the plugin's control thread has ended".into()))
    }

    /// How this plugin's window can be shown in `session`.
    pub fn plan(&self, session: GuiSession) -> GuiPlan {
        self.call(move |ctx| plan(ctx, session))
            .unwrap_or(GuiPlan::Parameters)
    }

    /// Opens the window as `plan` says and returns its size, when the plugin reports one.
    /// `parent` is the window to embed into, or to stay above for a floating window.
    ///
    /// Contract: the caller must call [`Self::close`] before it destroys `parent`.
    pub fn open(
        &self,
        plan: GuiPlan,
        parent: Option<ParentWindow>,
        scale: f64,
    ) -> Result<Option<(u32, u32)>, PluginError> {
        self.call(move |ctx| open(ctx, plan, parent, scale))?
    }

    /// Tells an embedded plugin its container is now `width` by `height`.
    pub fn resize(&self, width: u32, height: u32) -> Result<(), PluginError> {
        self.call(move |ctx| resize(ctx, width, height))?
    }

    /// The size an embedded plugin asked for since the last call, to apply to the container (then
    /// call [`Self::resize`]).
    pub fn take_resize_request(&self) -> Option<(u32, u32)> {
        let signals = &self.worker.signals;
        if !signals.gui_resize_pending.swap(false, Ordering::AcqRel) {
            return None;
        }
        let size = GuiSize::unpack_from_u64(signals.gui_resize.load(Ordering::Acquire));
        Some((size.width, size.height))
    }

    /// True once when the plugin closed its own window (a floating window's close button).
    pub fn take_closed(&self) -> bool {
        self.worker.signals.gui_closed.swap(false, Ordering::AcqRel)
    }

    /// Hides and destroys the window. Safe to call when none is open.
    pub fn close(&self) {
        let _ = self.call(destroy);
    }
}

fn gui_err(error: GuiError) -> PluginError {
    PluginError::Rejected(format!("plugin window: {error}"))
}

fn plan(ctx: &mut Ctx, session: GuiSession) -> GuiPlan {
    let handle = ctx.instance.plugin_handle();
    let Some(ext) = handle.get_extension::<GuiExt>() else {
        return GuiPlan::Parameters;
    };
    let preferred = ext
        .get_preferred_api(&handle)
        .and_then(|c| GuiApi::of(c.api_type).map(|api| (api, c.is_floating)));
    choose_plan(session, preferred, |api, is_floating| {
        let configuration = GuiConfiguration {
            api_type: api.clap_type(),
            is_floating,
        };
        ext.is_api_supported(&handle, configuration)
    })
}

fn open(
    ctx: &mut Ctx,
    plan: GuiPlan,
    parent: Option<ParentWindow>,
    scale: f64,
) -> Result<Option<(u32, u32)>, PluginError> {
    destroy(ctx);
    let (api, floating) = match plan {
        GuiPlan::Embedded(api) => (api, false),
        GuiPlan::Floating(api) => (api, true),
        GuiPlan::Parameters => {
            return Err(PluginError::Rejected(
                "this plugin's window cannot be shown here; use its parameter panel".into(),
            ))
        }
    };
    if !floating && parent.is_none() {
        return Err(PluginError::Rejected(
            "embedding needs a parent window".into(),
        ));
    }
    let handle = ctx.instance.plugin_handle();
    let ext = handle
        .get_extension::<GuiExt>()
        .ok_or_else(|| PluginError::Rejected("the plugin has no window".into()))?;
    let configuration = GuiConfiguration {
        api_type: api.clap_type(),
        is_floating: floating,
    };
    ext.create(&handle, configuration).map_err(gui_err)?;
    match show(&ext, &handle, floating, parent, scale) {
        Ok(size) => {
            ctx.gui = Some(ActiveGui { ext, floating });
            Ok(size)
        }
        Err(error) => {
            ext.destroy(&handle);
            Err(error)
        }
    }
}

/// The calls after `create`, in the order CLAP requires.
fn show(
    ext: &GuiExt,
    handle: &PluginMainThreadHandle,
    floating: bool,
    parent: Option<ParentWindow>,
    scale: f64,
) -> Result<Option<(u32, u32)>, PluginError> {
    // Plugins that size in logical pixels refuse a scale; that is not an error.
    let _ = ext.set_scale(handle, scale);
    match (floating, parent) {
        (false, Some(parent)) => {
            super::gui_parent::set_parent(ext, handle, parent.to_clap()).map_err(gui_err)?
        }
        (true, Some(parent)) => {
            // A floating window may refuse to be transient; it still opens, just not pinned above.
            let _ = super::gui_parent::set_transient(ext, handle, parent.to_clap());
        }
        _ => {}
    }
    let size = ext.get_size(handle).map(|s| (s.width, s.height));
    ext.show(handle).map_err(gui_err)?;
    Ok(size)
}

fn resize(ctx: &mut Ctx, width: u32, height: u32) -> Result<(), PluginError> {
    let Some(gui) = &ctx.gui else {
        return Err(PluginError::Rejected("no plugin window is open".into()));
    };
    let ext = gui.ext;
    let handle = ctx.instance.plugin_handle();
    let wanted = GuiSize { width, height };
    let size = ext.adjust_size(&handle, wanted).unwrap_or(wanted);
    ext.set_size(&handle, size).map_err(gui_err)
}

/// Hides and destroys the open window, if any. Control thread.
pub(super) fn destroy(ctx: &mut Ctx) {
    let Some(gui) = ctx.gui.take() else { return };
    let handle = ctx.instance.plugin_handle();
    let _ = gui.ext.hide(&handle);
    gui.ext.destroy(&handle);
}

/// Per-tick work for an open window: a floating plugin's resize request is acknowledged here (it
/// owns its window); an embedded one waits for the app to resize its container.
pub(super) fn service(ctx: &mut Ctx) {
    let Some(gui) = &ctx.gui else { return };
    if !gui.floating || !ctx.signals.gui_resize_pending.swap(false, Ordering::AcqRel) {
        return;
    }
    let size = GuiSize::unpack_from_u64(ctx.signals.gui_resize.load(Ordering::Acquire));
    let ext = gui.ext;
    let handle = ctx.instance.plugin_handle();
    let _ = ext.set_size(&handle, size);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_for(
        session: GuiSession,
        preferred: Option<(GuiApi, bool)>,
        ok: &[(GuiApi, bool)],
    ) -> GuiPlan {
        choose_plan(session, preferred, |api, floating| {
            ok.contains(&(api, floating))
        })
    }

    const X11_EMBED: (GuiApi, bool) = (GuiApi::X11, false);
    const X11_FLOAT: (GuiApi, bool) = (GuiApi::X11, true);
    const WL_FLOAT: (GuiApi, bool) = (GuiApi::Wayland, true);

    #[test]
    fn x11_embeds_when_it_can_and_floats_when_it_cannot() {
        assert_eq!(
            plan_for(GuiSession::X11, None, &[X11_EMBED, X11_FLOAT]),
            GuiPlan::Embedded(GuiApi::X11)
        );
        assert_eq!(
            plan_for(GuiSession::X11, None, &[X11_FLOAT]),
            GuiPlan::Floating(GuiApi::X11)
        );
        assert_eq!(
            plan_for(GuiSession::X11, None, &[WL_FLOAT]),
            GuiPlan::Parameters
        );
    }

    #[test]
    fn the_plugins_preference_wins_between_supported_options() {
        let both = [X11_EMBED, X11_FLOAT];
        assert_eq!(
            plan_for(GuiSession::X11, Some(X11_FLOAT), &both),
            GuiPlan::Floating(GuiApi::X11)
        );
        // A preference the plugin does not support is not chosen.
        assert_eq!(
            plan_for(GuiSession::X11, Some(X11_FLOAT), &[X11_EMBED]),
            GuiPlan::Embedded(GuiApi::X11)
        );
    }

    #[test]
    fn wayland_floats_only_for_wayland_plugins_and_x11_only_plugins_get_the_panel() {
        assert_eq!(
            plan_for(GuiSession::Wayland, None, &[WL_FLOAT]),
            GuiPlan::Floating(GuiApi::Wayland)
        );
        assert_eq!(
            plan_for(
                GuiSession::Wayland,
                Some(X11_EMBED),
                &[X11_EMBED, X11_FLOAT]
            ),
            GuiPlan::Parameters
        );
        assert_eq!(
            plan_for(GuiSession::Other, None, &[X11_EMBED, WL_FLOAT]),
            GuiPlan::Parameters
        );
    }

    #[test]
    fn raw_handles_map_to_sessions_and_parents() {
        use raw_window_handle::{WaylandWindowHandle, XcbWindowHandle};
        use std::num::NonZeroU32;
        use std::ptr::NonNull;
        let xcb = RawWindowHandle::Xcb(XcbWindowHandle::new(
            NonZeroU32::new(0x40_0007).expect("nonzero"),
        ));
        assert_eq!(GuiSession::of(&xcb), GuiSession::X11);
        assert_eq!(ParentWindow::of(&xcb), Some(ParentWindow::X11(0x40_0007)));
        let wl =
            RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::<u8>::dangling().cast()));
        assert_eq!(GuiSession::of(&wl), GuiSession::Wayland);
        assert_eq!(ParentWindow::of(&wl), None);
    }
}
