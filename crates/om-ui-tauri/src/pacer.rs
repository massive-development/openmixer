//! Fixed-rate pacing for the control-side loops (the stream and the deck loop). Owned by the
//! thread that ticks it.

use std::time::{Duration, Instant};

/// The rate a non-finite request falls back to.
const FALLBACK_HZ: f64 = 30.0;
/// Rates outside this range are clamped, so a period is never zero or hours long.
const HZ_RANGE: (f64, f64) = (1.0, 1000.0);

/// A deadline advanced by one period per tick and reset when the loop falls behind, so a late
/// tick never turns into a burst of catch-up ticks.
pub(crate) struct Pacer {
    period: Duration,
    next: Instant,
}

impl Pacer {
    pub(crate) fn new(hz: f64) -> Self {
        Self {
            period: period_of(hz),
            next: Instant::now(),
        }
    }

    /// Sleeps until the next tick is due; returns at once when it is already late.
    pub(crate) fn wait(&mut self) {
        self.next += self.period;
        let now = Instant::now();
        if self.next <= now {
            self.next = now;
            return;
        }
        std::thread::sleep(self.next - now);
    }
}

fn period_of(hz: f64) -> Duration {
    let hz = if hz.is_finite() {
        hz.clamp(HZ_RANGE.0, HZ_RANGE.1)
    } else {
        FALLBACK_HZ
    };
    Duration::from_secs_f64(1.0 / hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_rates_get_a_sane_period() {
        assert_eq!(period_of(f64::NAN), period_of(FALLBACK_HZ));
        assert_eq!(period_of(0.0), Duration::from_secs(1));
        assert_eq!(period_of(-5.0), Duration::from_secs(1));
        assert_eq!(period_of(1e9), Duration::from_millis(1));
    }

    #[test]
    fn a_late_tick_resets_instead_of_bursting() {
        let mut pacer = Pacer::new(100.0);
        std::thread::sleep(Duration::from_millis(60));
        let start = Instant::now();
        pacer.wait();
        assert!(start.elapsed() < Duration::from_millis(5), "late: no sleep");
        pacer.wait();
        assert!(
            start.elapsed() >= Duration::from_millis(9),
            "then one period"
        );
    }
}
