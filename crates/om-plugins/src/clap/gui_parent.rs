//! The two `unsafe` GUI calls: clack makes `set_parent` and `set_transient` unsafe because it cannot
//! check that the window handle is still valid (ADR 0007 assumed they were safe; they are not in 0.2.0).

#![allow(unsafe_code)]

use clack_extensions::gui::{GuiError, PluginGui, Window};
use clack_host::prelude::PluginMainThreadHandle;

/// Embeds the plugin's GUI into `window`.
///
/// The caller (`gui::open`) only reaches this with a window id the app handed to
/// `PluginGui::open`, and the contract there is that the app calls `PluginGui::close` before it
/// destroys that window (the window view does so in its `Drop`).
pub(super) fn set_parent(
    ext: &PluginGui,
    plugin: &PluginMainThreadHandle,
    window: Window,
) -> Result<(), GuiError> {
    // SAFETY: the parent window outlives the plugin GUI by the `close`-before-destroy contract
    // above; clack's requirement is exactly that the window stays valid until `destroy`.
    unsafe { ext.set_parent(plugin, window) }
}

/// Asks a floating GUI to stay above `window`.
pub(super) fn set_transient(
    ext: &PluginGui,
    plugin: &PluginMainThreadHandle,
    window: Window,
) -> Result<(), GuiError> {
    // SAFETY: same contract as `set_parent`: the transient-for window is closed after the GUI.
    unsafe { ext.set_transient(plugin, window) }
}
