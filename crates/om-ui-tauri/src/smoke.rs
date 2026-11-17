//! The smoke and bench runs' watchdog and verdict. The watchdog thread ends a run whose front end
//! never reports (ready, or the bench result), so CI fails instead of hanging.

use crate::ShellError;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use tauri::AppHandle;

/// How long a smoke run waits for the front end's `ready` before it fails.
pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a bench run of `seconds` waits for the front end's result: the smoke wait on top.
pub fn bench_timeout(seconds: u32) -> Duration {
    SMOKE_TIMEOUT + Duration::from_secs(u64::from(seconds))
}

/// Exits the app with code 1 unless `signal` arrives within `timeout`.
pub(crate) fn spawn_watchdog(
    app: AppHandle,
    signal: Receiver<()>,
    timeout: Duration,
) -> Result<(), ShellError> {
    let wait = move || {
        if let Err(RecvTimeoutError::Timeout) = signal.recv_timeout(timeout) {
            tracing::error!(seconds = timeout.as_secs(), "the front end never reported");
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
    fn a_bench_waits_its_length_on_top_of_the_smoke_wait() {
        assert_eq!(bench_timeout(20), Duration::from_secs(80));
    }

    #[test]
    fn a_non_zero_exit_is_an_error() {
        assert!(matches!(outcome(false, 3), Err(ShellError::Exit(3))));
    }
}
