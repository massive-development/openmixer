//! The Tauri commands the front end calls. Tauri runs them on its IPC threads, never the audio thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// What the commands share for the life of the window.
pub(crate) struct ShellState {
    pub(crate) theme_css: String,
    pub(crate) smoke: bool,
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) ready_signal: Sender<()>,
}

/// The active theme as one `:root` block of `--om-*` variables.
#[tauri::command]
pub fn theme_css(state: State<'_, ShellState>) -> String {
    state.theme_css.clone()
}

/// Called by the front end after its first paint; a smoke run ends here with code 0.
#[tauri::command]
pub fn ready(app: AppHandle, state: State<'_, ShellState>) {
    state.ready.store(true, Ordering::Release);
    // The smoke watchdog may already have given up; nothing else listens.
    let _ = state.ready_signal.send(());
    tracing::info!(smoke = state.smoke, "front end ready");
    if state.smoke {
        app.exit(0);
    }
}
