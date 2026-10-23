//! A smoothed playhead. The engine publishes its position once per audio callback, so the raw value
//! moves in steps of one device buffer (several milliseconds, uneven against the frame times).
//! Drawn directly, the zoomed strip scrolls in visible jerks. [`PlayheadClock`] instead runs on the
//! monotonic frame clock at the playback speed and is nudged toward each new published position, so
//! the motion is even and still follows the engine. Pure: the caller passes the time.

/// A published position further than this many seconds from the prediction is a seek, loop jump or
/// stall, and the clock snaps to it instead of gliding.
const SNAP_SECONDS: f64 = 0.25;
/// How much of the error between the prediction and a fresh published position is taken up at
/// once. Small, so buffer-sized steps are absorbed rather than shown.
const CORRECTION: f64 = 0.15;

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayheadClock {
    /// Position in file frames at `at`.
    frame: f64,
    /// Seconds on the caller's monotonic clock when `frame` was current.
    at: f64,
    /// The last raw position seen, to notice when the engine publishes a new one.
    raw: i64,
    started: bool,
}

impl PlayheadClock {
    /// Feeds the engine's published position and returns the smoothed position at `now` (seconds,
    /// monotonic). `frames_per_second` is the file frames played per second (rate times file rate);
    /// zero or `playing == false` pins the clock to the raw position.
    pub fn advance(&mut self, raw: i64, now: f64, playing: bool, frames_per_second: f64) -> f64 {
        if !self.started || !playing || frames_per_second <= 0.0 {
            *self = Self {
                frame: raw as f64,
                at: now,
                raw,
                started: true,
            };
            return self.frame;
        }
        let predicted = self.frame + (now - self.at).max(0.0) * frames_per_second;
        let next = if raw == self.raw {
            predicted
        } else {
            let error = raw as f64 - predicted;
            if error.abs() > SNAP_SECONDS * frames_per_second {
                raw as f64
            } else {
                predicted + error * CORRECTION
            }
        };
        self.frame = next;
        self.at = now;
        self.raw = raw;
        next
    }

    /// The position from the last [`advance`](Self::advance); `None` before the first one.
    pub fn current(&self) -> Option<f64> {
        self.started.then_some(self.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: f64 = 44_100.0;

    /// Raw positions the way the audio thread publishes them: stepped once per 1024-frame buffer.
    fn published(t: f64) -> i64 {
        ((t * FPS / 1024.0).floor() * 1024.0) as i64
    }

    #[test]
    fn steady_playback_moves_evenly_between_buffer_steps() {
        let mut clock = PlayheadClock::default();
        let mut last = clock.advance(published(0.0), 0.0, true, FPS);
        let mut worst_raw = 0.0f64;
        let mut worst_smooth = 0.0f64;
        let mut last_raw = published(0.0);
        for i in 1..600 {
            let t = i as f64 / 60.0;
            let raw = published(t);
            let p = clock.advance(raw, t, true, FPS);
            worst_raw = worst_raw.max(((raw - last_raw) as f64 - FPS / 60.0).abs());
            worst_smooth = worst_smooth.max(((p - last) - FPS / 60.0).abs());
            assert!(p >= last, "never runs backwards");
            assert!(
                (p - t * FPS).abs() < 1024.0,
                "stays within a buffer of the engine"
            );
            last = p;
            last_raw = raw;
        }
        assert!(
            worst_smooth < worst_raw / 3.0,
            "smooth {worst_smooth} raw {worst_raw}"
        );
    }

    #[test]
    fn seeks_snap_and_pausing_pins_to_the_engine() {
        let mut clock = PlayheadClock::default();
        clock.advance(0, 0.0, true, FPS);
        clock.advance(1024, 0.02, true, FPS);
        assert_eq!(clock.advance(10_000_000, 0.04, true, FPS), 10_000_000.0);
        assert_eq!(clock.advance(10_000_000, 1.0, false, FPS), 10_000_000.0);
        assert_eq!(clock.current(), Some(10_000_000.0));
    }
}
