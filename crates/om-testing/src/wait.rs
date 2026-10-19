//! Condition-based waits. Tests wait for the thing they expect to happen, not for a fixed time
//! that is too short on a loaded machine and too long everywhere else.

use std::time::{Duration, Instant};

/// Poll interval used by [`assert_eventually!`].
pub const DEFAULT_POLL: Duration = Duration::from_millis(2);

/// Polls `cond` every `poll` until it returns true or `timeout` elapses. Returns whether the
/// condition held. The condition is checked once more at the deadline, so a `timeout` of zero
/// still checks once.
pub fn wait_until(timeout: Duration, poll: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        if poll.is_zero() {
            std::thread::yield_now();
        } else {
            std::thread::sleep(poll.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
}

/// Panics unless the condition becomes true within the timeout. Polls every [`DEFAULT_POLL`].
///
/// ```text
/// assert_eventually!(Duration::from_secs(5), || counter.load(Ordering::Relaxed) >= 3);
/// assert_eventually!(Duration::from_secs(5), || ready(), "engine never became ready");
/// ```
#[macro_export]
macro_rules! assert_eventually {
    ($timeout:expr, $cond:expr $(,)?) => {
        $crate::assert_eventually!($timeout, $cond, "condition still false after timeout")
    };
    ($timeout:expr, $cond:expr, $($msg:tt)+) => {{
        let timeout: ::std::time::Duration = $timeout;
        if !$crate::wait::wait_until(timeout, $crate::wait::DEFAULT_POLL, $cond) {
            panic!("{} (waited {:?})", format_args!($($msg)+), timeout);
        }
    }};
}
