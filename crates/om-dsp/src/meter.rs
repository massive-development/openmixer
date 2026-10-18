//! Peak metering. [`PeakMeter`] is a decaying peak an audio thread writes and anyone can read.
//! [`MeterBallistics`] shapes linear peaks for display: instant attack, exponential release, a
//! peak-hold tick and a latching clip flag. It is fed one peak per frame with the frame time, so it is
//! frame-rate independent and testable with synthetic levels.

use std::sync::atomic::{AtomicU32, Ordering};

/// Decaying peak level written by the audio thread and read by any thread.
#[derive(Debug, Default)]
pub struct PeakMeter {
    value: AtomicU32,
}

impl PeakMeter {
    pub fn push(&self, block_peak: f32, decay: f32) {
        let next = block_peak.max(self.get() * decay);
        self.value.store(next.to_bits(), Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.value.store(0.0_f32.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.value.load(Ordering::Relaxed))
    }
}

/// Display ballistics parameters. Defaults match the C++ theme fallbacks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterParams {
    pub floor_db: f32,
    pub clip_level: f32,
    pub release_seconds: f32,
    pub hold_seconds: f32,
    pub clip_seconds: f32,
}

impl Default for MeterParams {
    fn default() -> Self {
        Self {
            floor_db: -48.0,
            clip_level: 0.99,
            release_seconds: 0.3,
            hold_seconds: 1.0,
            clip_seconds: 2.0,
        }
    }
}

const SNAP_TO_ZERO: f32 = 0.002;

#[derive(Debug, Clone, Copy, Default)]
pub struct MeterBallistics {
    params: MeterParams,
    level_proportion: f32,
    peak_proportion: f32,
    hold_remaining: f32,
    clip_remaining: f32,
    clipping: bool,
}

impl MeterBallistics {
    pub fn new(params: MeterParams) -> Self {
        Self {
            params,
            ..Self::default()
        }
    }

    /// Maps a linear level onto a 0..=1 bar height over `floor_db..0 dB`.
    pub fn proportion_of(linear: f32, floor_db: f32) -> f32 {
        if linear <= 0.0 || floor_db >= 0.0 {
            return 0.0;
        }
        ((20.0 * linear.log10() - floor_db) / -floor_db).clamp(0.0, 1.0)
    }

    /// Advances the meter by `dt_seconds` with a new linear peak. Returns true when anything visible changed.
    pub fn update(&mut self, linear_peak: f32, dt_seconds: f32) -> bool {
        let before = (self.level_proportion, self.peak_proportion, self.clipping);

        let target = Self::proportion_of(linear_peak, self.params.floor_db);
        let fall = (-dt_seconds.max(0.0) / self.params.release_seconds.max(1e-3)).exp();
        self.level_proportion = target.max(self.level_proportion * fall);
        if self.level_proportion < SNAP_TO_ZERO {
            self.level_proportion = 0.0;
        }

        if target >= self.peak_proportion {
            self.peak_proportion = target;
            self.hold_remaining = self.params.hold_seconds;
        } else if self.hold_remaining > 0.0 {
            self.hold_remaining -= dt_seconds;
        } else {
            self.peak_proportion = self.level_proportion.max(self.peak_proportion * fall);
        }

        self.update_clip(linear_peak, dt_seconds);
        before != (self.level_proportion, self.peak_proportion, self.clipping)
    }

    pub fn reset_clip(&mut self) {
        self.clip_remaining = 0.0;
        self.clipping = false;
    }

    /// Bar height, 0..=1.
    pub fn level(&self) -> f32 {
        self.level_proportion
    }

    /// Hold tick height, 0..=1.
    pub fn peak_hold(&self) -> f32 {
        self.peak_proportion
    }

    pub fn is_clipping(&self) -> bool {
        self.clipping
    }

    pub fn params(&self) -> &MeterParams {
        &self.params
    }

    pub fn set_params(&mut self, params: MeterParams) {
        self.params = params;
    }

    fn update_clip(&mut self, linear_peak: f32, dt: f32) {
        if linear_peak >= self.params.clip_level {
            self.clip_remaining = self.params.clip_seconds;
        } else {
            self.clip_remaining = (self.clip_remaining - dt).max(0.0);
        }
        self.clipping = linear_peak >= self.params.clip_level || self.clip_remaining > 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::{MeterBallistics, MeterParams, PeakMeter};

    const FRAME: f32 = 1.0 / 30.0;

    fn params() -> MeterParams {
        MeterParams {
            floor_db: -48.0,
            clip_level: 0.99,
            release_seconds: 0.5,
            hold_seconds: 1.0,
            clip_seconds: 2.0,
        }
    }

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn proportion_maps_db_onto_the_scale() {
        assert_eq!(MeterBallistics::proportion_of(0.0, -48.0), 0.0);
        assert!(approx(
            MeterBallistics::proportion_of(1.0, -48.0),
            1.0,
            1e-6
        ));
        assert!(
            approx(MeterBallistics::proportion_of(0.0630957, -48.0), 0.5, 1e-3),
            "-24 dB is mid-scale"
        );
        assert_eq!(
            MeterBallistics::proportion_of(1e-6, -48.0),
            0.0,
            "below the floor"
        );
        assert_eq!(MeterBallistics::proportion_of(4.0, -48.0), 1.0, "clamped");
    }

    #[test]
    fn attack_is_instant_and_release_is_exponential() {
        let mut m = MeterBallistics::new(params());
        m.update(0.5, FRAME);
        let top = m.level();
        assert!(approx(
            top,
            MeterBallistics::proportion_of(0.5, -48.0),
            1e-6
        ));

        m.update(0.0, 0.5); // one release time constant
        assert!(approx(m.level(), top * (-1.0_f32).exp(), 1e-4));
        for _ in 0..300 {
            m.update(0.0, FRAME);
        }
        assert_eq!(m.level(), 0.0, "snaps to zero instead of lingering");
    }

    #[test]
    fn release_is_frame_rate_independent() {
        let mut a = MeterBallistics::new(params());
        let mut b = MeterBallistics::new(params());
        a.update(0.5, FRAME);
        b.update(0.5, FRAME);
        a.update(0.0, 0.2);
        for _ in 0..4 {
            b.update(0.0, 0.05);
        }
        assert!(approx(a.level(), b.level(), 1e-4));
    }

    #[test]
    fn peak_hold_stays_then_decays() {
        let mut m = MeterBallistics::new(params());
        m.update(0.8, FRAME);
        let held = m.peak_hold();
        assert!(held > 0.9);

        for _ in 0..25 {
            // 0.83 s < hold time
            m.update(0.0, FRAME);
        }
        assert_eq!(m.peak_hold(), held, "still holding");
        assert!(m.level() < held);

        for _ in 0..10 {
            // past the hold time
            m.update(0.0, FRAME);
        }
        assert!(m.peak_hold() < held, "falls once the hold expires");

        m.update(0.9, FRAME); // a new higher peak re-arms the hold
        assert!(m.peak_hold() > held);
    }

    #[test]
    fn lower_peak_does_not_lower_the_held_tick() {
        let mut m = MeterBallistics::new(params());
        m.update(0.8, FRAME);
        let held = m.peak_hold();
        m.update(0.2, FRAME);
        assert_eq!(m.peak_hold(), held);
    }

    #[test]
    fn clip_latches_for_its_hold_time_and_can_be_cleared() {
        let mut m = MeterBallistics::new(params());
        assert!(!m.is_clipping());
        m.update(1.0, FRAME);
        assert!(m.is_clipping());
        for _ in 0..30 {
            // 1 s later, silent: still lit
            m.update(0.0, FRAME);
        }
        assert!(m.is_clipping());
        for _ in 0..40 {
            // 2.3 s total: gone
            m.update(0.0, FRAME);
        }
        assert!(!m.is_clipping());

        m.update(1.2, FRAME);
        m.reset_clip();
        assert!(!m.is_clipping());
        m.update(0.5, FRAME);
        assert!(!m.is_clipping());
    }

    #[test]
    fn update_reports_whether_anything_changed() {
        let mut m = MeterBallistics::new(params());
        assert!(
            !m.update(0.0, FRAME),
            "silence on a silent meter needs no repaint"
        );
        assert!(m.update(0.5, FRAME));
        assert!(m.update(0.0, FRAME));
    }

    #[test]
    fn peak_meter_jumps_up_and_decays_by_the_factor() {
        let meter = PeakMeter::default();
        meter.push(0.8, 0.5);
        assert_eq!(meter.get(), 0.8);
        meter.push(0.0, 0.5);
        assert!(approx(meter.get(), 0.4, 1e-6));
        meter.reset();
        assert_eq!(meter.get(), 0.0);
    }
}
