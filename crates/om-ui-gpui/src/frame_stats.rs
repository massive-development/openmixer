//! Frame-time measurement for smoothness work (`OPENMIXER_FRAME_STATS=1`). Records the time between
//! frames and the time the view takes to build each one, and reports percentiles every few seconds.
//! Pure bookkeeping: the caller passes the clock, so the numbers are testable.

use std::time::{Duration, Instant};

/// How often a report is produced.
pub const REPORT_EVERY: Duration = Duration::from_secs(5);
/// A frame later than this after the previous one counts as a visible hitch.
const HITCH_MS: f32 = 25.0;

/// Percentiles of one series, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
}

/// Percentiles of `values` (sorted in place). All zero for an empty series.
pub fn summarize(values: &mut [f32]) -> Summary {
    if values.is_empty() {
        return Summary {
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            max: 0.0,
        };
    }
    values.sort_by(f32::total_cmp);
    let at = |q: f32| values[((values.len() - 1) as f32 * q).round() as usize];
    Summary {
        p50: at(0.50),
        p95: at(0.95),
        p99: at(0.99),
        max: values[values.len() - 1],
    }
}

#[derive(Debug, Default)]
pub struct FrameStats {
    last: Option<Instant>,
    window_start: Option<Instant>,
    intervals_ms: Vec<f32>,
    builds_ms: Vec<f32>,
}

impl FrameStats {
    /// Records one frame that started at `now` and took `build` to build. Returns a report line
    /// once [`REPORT_EVERY`] has passed since the last one.
    pub fn frame(&mut self, now: Instant, build: Duration) -> Option<String> {
        if let Some(last) = self.last {
            self.intervals_ms.push((now - last).as_secs_f32() * 1000.0);
        }
        self.last = Some(now);
        self.builds_ms.push(build.as_secs_f32() * 1000.0);
        let start = *self.window_start.get_or_insert(now);
        if now - start < REPORT_EVERY {
            return None;
        }
        let report = self.report(now - start);
        self.intervals_ms.clear();
        self.builds_ms.clear();
        self.window_start = Some(now);
        Some(report)
    }

    fn report(&mut self, span: Duration) -> String {
        let hitches = self.intervals_ms.iter().filter(|m| **m > HITCH_MS).count();
        let fps = self.builds_ms.len() as f32 / span.as_secs_f32().max(f32::EPSILON);
        let i = summarize(&mut self.intervals_ms);
        let b = summarize(&mut self.builds_ms);
        format!(
            "frames {fps:.1}/s | interval ms p50 {:.1} p95 {:.1} p99 {:.1} max {:.1} | build ms p50 {:.2} p95 {:.2} p99 {:.2} max {:.2} | hitches>{HITCH_MS}ms {hitches}",
            i.p50, i.p95, i.p99, i.max, b.p50, b.p95, b.p99, b.max
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_a_known_series() {
        let mut v: Vec<f32> = (1..=100).map(|n| n as f32).collect();
        let s = summarize(&mut v);
        assert_eq!((s.p50, s.max), (51.0, 100.0));
        assert!(s.p95 >= 95.0 && s.p99 >= 99.0);
        assert_eq!(summarize(&mut []).max, 0.0);
    }

    #[test]
    fn reports_once_per_window_and_counts_hitches() {
        let mut stats = FrameStats::default();
        let t0 = Instant::now();
        let mut reports = Vec::new();
        for n in 0..600u32 {
            // 60 Hz with one 60 ms stall.
            let at = t0
                + Duration::from_micros(16_667 * u64::from(n) + if n > 100 { 60_000 } else { 0 });
            if let Some(r) = stats.frame(at, Duration::from_micros(900)) {
                reports.push(r);
            }
        }
        assert_eq!(
            reports.len(),
            2,
            "ten seconds of frames, two five-second windows"
        );
        assert!(reports[0].contains("hitches>25ms 1"), "{}", reports[0]);
        assert!(reports[1].contains("hitches>25ms 0"), "{}", reports[1]);
    }
}
