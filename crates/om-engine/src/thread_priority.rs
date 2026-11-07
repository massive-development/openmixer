//! Real-time scheduling for engine worker threads, through `audio_thread_priority`. Control
//! thread side: promotion allocates and may talk to D-Bus, so it runs once when a worker starts,
//! never on the audio callback. Refusal is soft: it is logged and the thread runs at normal
//! priority.

use audio_thread_priority::{
    demote_current_thread_from_real_time, promote_current_thread_to_real_time, RtPriorityHandle,
};

/// Keeps the calling thread promoted until dropped, then restores normal scheduling.
#[must_use = "the thread is demoted when the guard drops"]
pub(crate) struct RealTimeGuard {
    role: &'static str,
    handle: Option<RtPriorityHandle>,
}

impl RealTimeGuard {
    /// Promotes the current thread. `budget_frames` at `sample_rate` is the longest burst of CPU
    /// the thread may use between blocking calls: on Linux it becomes the `RLIMIT_RTTIME` soft
    /// limit, and exceeding it raises `SIGXCPU`, so callers pass the worst burst they can make.
    /// Never fails: a refusal is logged and the guard is inert.
    pub(crate) fn promote(role: &'static str, budget_frames: u32, sample_rate: u32) -> Self {
        let handle = match promote_current_thread_to_real_time(budget_frames, sample_rate) {
            Ok(handle) => {
                tracing::debug!(role, "thread promoted to real-time priority");
                Some(handle)
            }
            Err(error) => {
                tracing::warn!(
                    role,
                    %error,
                    "real-time priority refused; thread keeps normal priority"
                );
                None
            }
        };
        Self { role, handle }
    }

    #[cfg(test)]
    fn is_promoted(&self) -> bool {
        self.handle.is_some()
    }
}

impl Drop for RealTimeGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            if let Err(error) = demote_current_thread_from_real_time(handle) {
                tracing::debug!(role = self.role, %error, "could not restore normal priority");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotion_never_panics_whether_or_not_the_os_allows_it() {
        let guard = RealTimeGuard::promote("test", 4096, 48_000);
        let _ = guard.is_promoted();
        drop(guard);
    }

    #[test]
    fn zero_sample_rate_is_refused_softly() {
        let guard = RealTimeGuard::promote("test", 0, 0);
        assert!(!guard.is_promoted());
    }
}
