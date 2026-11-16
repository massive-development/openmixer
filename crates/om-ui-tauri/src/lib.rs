#![forbid(unsafe_code)]

//! The Tauri v2 window of the web front end trial (ADR 0009). It owns the window and the Tauri
//! commands; the composition (engine, router, theme) comes from the `openmixer` binary.
//! [`run`] runs on the main thread; the commands run on Tauri's IPC threads.

mod commands;

use commands::ShellState;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;
use tauri::{App, AppHandle};

/// How long a smoke run waits for the front end's `ready` before it fails.
pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(60);

/// How the window runs.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Exit with code 0 once the front end has painted and called `ready`.
    pub smoke: bool,
    /// The notice the window shows (the mock device); the empty shell does not show it yet.
    pub banner: Option<String>,
    /// Frame benchmark length (the trial's measurement task); unused until then.
    pub bench_seconds: Option<u32>,
}

/// Why the window could not run, or a smoke run failed.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("could not start the smoke watchdog: {0}")]
    Watchdog(std::io::Error),
    #[error("the front end never called ready (is web/dist built?)")]
    NotReady,
    #[error("the window exited with code {0}")]
    Exit(i32),
}

/// Opens the window over `engine` and `router`, styled by `theme_css`, and returns when it closes.
pub fn run(
    engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    theme_css: String,
    options: RunOptions,
) -> Result<(), ShellError> {
    disable_dmabuf_renderer();
    let (ready_signal, ready_wait) = mpsc::channel();
    let ready = Arc::new(AtomicBool::new(false));
    let state = ShellState {
        theme_css,
        smoke: options.smoke,
        ready: Arc::clone(&ready),
        ready_signal,
    };
    let app = build_app(engine, router, state)?;
    if options.smoke {
        spawn_watchdog(app.handle().clone(), ready_wait)?;
    }
    let code = app.run_return(|_, _| {});
    outcome(options.smoke && !ready.load(Ordering::Acquire), code)
}

fn build_app(
    engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    state: ShellState,
) -> Result<App, ShellError> {
    let app = tauri::Builder::default()
        .manage(engine)
        .manage(router)
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::theme_css,
            commands::ready
        ])
        .build(tauri::generate_context!())?;
    Ok(app)
}

/// Ends a smoke run whose front end never reports ready, so CI fails instead of hanging.
fn spawn_watchdog(app: AppHandle, ready: Receiver<()>) -> Result<(), ShellError> {
    let wait = move || {
        if let Err(RecvTimeoutError::Timeout) = ready.recv_timeout(SMOKE_TIMEOUT) {
            tracing::error!(seconds = SMOKE_TIMEOUT.as_secs(), "smoke: no ready call");
            app.exit(1);
        }
    };
    let thread = std::thread::Builder::new().name("om-ui-smoke".into());
    thread.spawn(wait).map(drop).map_err(ShellError::Watchdog)
}

fn outcome(missed_ready: bool, code: i32) -> Result<(), ShellError> {
    match (missed_ready, code) {
        (true, _) => Err(ShellError::NotReady),
        (false, 0) => Ok(()),
        (false, code) => Err(ShellError::Exit(code)),
    }
}

/// ADR 0009: under WSL software rendering this cut WebKitGTK from 213% to 56% CPU.
#[cfg(target_os = "linux")]
fn disable_dmabuf_renderer() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VAR).is_none() {
        std::env::set_var(VAR, "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn disable_dmabuf_renderer() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_exit_is_ok() {
        assert!(outcome(false, 0).is_ok());
    }

    #[test]
    fn a_smoke_run_without_ready_fails_even_on_code_zero() {
        assert!(matches!(outcome(true, 0), Err(ShellError::NotReady)));
        assert!(matches!(outcome(true, 1), Err(ShellError::NotReady)));
    }

    #[test]
    fn a_non_zero_exit_is_an_error() {
        assert!(matches!(outcome(false, 3), Err(ShellError::Exit(3))));
    }
}
