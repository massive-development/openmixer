//! The smoke run's watchdog and verdict. The watchdog thread ends a run whose front end never
//! reports ready, so CI fails instead of hanging.

use crate::ShellError;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use tauri::AppHandle;

/// How long a smoke run waits for the front end's `ready` before it fails.
pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn spawn_watchdog(app: AppHandle, ready: Receiver<()>) -> Result<(), ShellError> {
    let wait = move || {
        if let Err(RecvTimeoutError::Timeout) = ready.recv_timeout(SMOKE_TIMEOUT) {
            tracing::error!(seconds = SMOKE_TIMEOUT.as_secs(), "smoke: no ready call");
            app.exit(1);
        }
    };
    let thread = std::thread::Builder::new().name("om-ui-smoke".into());
    thread.spawn(wait).map(drop).map_err(ShellError::Watchdog)
}

pub(crate) fn outcome(missed_ready: bool, code: i32) -> Result<(), ShellError> {
    match (missed_ready, code) {
        (true, _) => Err(ShellError::NotReady),
        (false, 0) => Ok(()),
        (false, code) => Err(ShellError::Exit(code)),
    }
}

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
