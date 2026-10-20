//! Four-stem to stereo mixer. A stem deck carries drums, bass, vocals and other as eight floats per
//! source frame (L/R of each stem); the mixer folds them into one stereo frame with per-stem gain,
//! mute and sweep filter. The deck calls it at source-frame pull time, before interpolation and
//! key-lock, so there is still one stretcher per deck.
//!
//! [`StemMixerControl`] is the shared control side (atomics, `Send + Sync`); [`StemMixer`] is the
//! audio side. Allocation-free and lock-free. Gains and mutes glide through one-pole smoothers so
//! a change never clicks; a filter at 0 is a bit-exact bypass.

use crate::{Smoother, SweepFilter};
use om_core::atomic::AtomicF32;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;

/// Stems per deck: drums, bass, vocals, other.
pub const STEM_COUNT: usize = 4;
/// Floats per source frame in stem mode: L/R for each stem.
pub const STEM_FRAME_FLOATS: usize = STEM_COUNT * 2;
/// Linear gain ceiling.
pub const MAX_STEM_GAIN: f32 = 1.5;

const GAIN_GLIDE_SECONDS: f64 = 0.010;
const MUTE_GLIDE_SECONDS: f64 = 0.008;
const SETTLE_EPSILON: f32 = 2.0e-5;

/// Control side: written from control threads, read by the audio-side mixer once per block.
#[derive(Debug)]
pub struct StemMixerControl {
    gains: [AtomicF32; STEM_COUNT],
    mutes: [AtomicBool; STEM_COUNT],
    filters: [AtomicF32; STEM_COUNT],
    available: AtomicBool,
}

impl Default for StemMixerControl {
    fn default() -> Self {
        Self::new()
    }
}

impl StemMixerControl {
    /// Unity gains, no mutes, filters off, stems unavailable.
    pub fn new() -> Self {
        Self {
            gains: std::array::from_fn(|_| AtomicF32::new(1.0)),
            mutes: std::array::from_fn(|_| AtomicBool::new(false)),
            filters: std::array::from_fn(|_| AtomicF32::new(0.0)),
            available: AtomicBool::new(false),
        }
    }

    /// Linear gain, clamped to `0..=MAX_STEM_GAIN`. NaN and an index past the stems are ignored.
    pub fn set_gain(&self, stem: usize, gain: f32) {
        if let (Some(cell), false) = (self.gains.get(stem), gain.is_nan()) {
            cell.store(gain.clamp(0.0, MAX_STEM_GAIN));
        }
    }

    pub fn gain(&self, stem: usize) -> f32 {
        self.gains.get(stem).map_or(1.0, AtomicF32::load)
    }

    pub fn set_muted(&self, stem: usize, muted: bool) {
        if let Some(cell) = self.mutes.get(stem) {
            cell.store(muted, Relaxed);
        }
    }

    pub fn is_muted(&self, stem: usize) -> bool {
        self.mutes.get(stem).is_some_and(|m| m.load(Relaxed))
    }

    /// Sweep position `-1..=1` (-1 low-pass, 0 off, +1 high-pass). NaN is ignored.
    pub fn set_filter(&self, stem: usize, position: f32) {
        if let (Some(cell), false) = (self.filters.get(stem), position.is_nan()) {
            cell.store(position.clamp(-1.0, 1.0));
        }
    }

    pub fn filter(&self, stem: usize) -> f32 {
        self.filters.get(stem).map_or(0.0, AtomicF32::load)
    }

    /// True while the deck plays from stems.
    pub fn is_available(&self) -> bool {
        self.available.load(Relaxed)
    }

    pub fn set_available(&self, available: bool) {
        self.available.store(available, Relaxed);
    }

    /// Back to defaults (unity, unmuted, filters off). Leaves availability alone.
    pub fn reset(&self) {
        for stem in 0..STEM_COUNT {
            self.set_gain(stem, 1.0);
            self.set_muted(stem, false);
            self.set_filter(stem, 0.0);
        }
    }
}

/// Steps a smoother and lands exactly on the target once within -94 dB of it. The one-pole alone
/// can stall a hair short of the target when its increment drops below the float resolution.
#[inline]
fn settle(smoother: &mut Smoother, target: f32) -> f32 {
    let v = smoother.step(target);
    if (v - target).abs() < SETTLE_EPSILON {
        smoother.snap(target);
        target
    } else {
        v
    }
}

/// Audio side. One per deck.
pub struct StemMixer {
    control: Arc<StemMixerControl>,
    gain: [Smoother; STEM_COUNT],
    audible: [Smoother; STEM_COUNT],
    filter: [SweepFilter; STEM_COUNT],
    target_gain: [f32; STEM_COUNT],
    target_audible: [f32; STEM_COUNT],
    filter_on: [bool; STEM_COUNT],
}

impl StemMixer {
    pub fn new(control: Arc<StemMixerControl>) -> Self {
        let mut mixer = Self {
            control,
            gain: [Smoother::default(); STEM_COUNT],
            audible: [Smoother::default(); STEM_COUNT],
            filter: std::array::from_fn(|_| SweepFilter::new()),
            target_gain: [1.0; STEM_COUNT],
            target_audible: [1.0; STEM_COUNT],
            filter_on: [false; STEM_COUNT],
        };
        mixer.prepare(44_100.0);
        mixer
    }

    /// Sets the rate the frames arrive at (the file's rate) and snaps to the control state. Does
    /// not allocate; the deck calls it when it switches into stem mode.
    pub fn prepare(&mut self, sample_rate: f64) {
        for stem in 0..STEM_COUNT {
            self.gain[stem].set_time(sample_rate, GAIN_GLIDE_SECONDS);
            self.audible[stem].set_time(sample_rate, MUTE_GLIDE_SECONDS);
            self.filter[stem].prepare(sample_rate);
        }
        self.update_targets();
        self.snap();
    }

    /// Jumps the smoothers to their targets and clears the filter memory.
    pub fn snap(&mut self) {
        for stem in 0..STEM_COUNT {
            self.gain[stem].snap(self.target_gain[stem]);
            self.audible[stem].snap(self.target_audible[stem]);
            self.filter[stem].reset();
        }
    }

    /// Reads the control atomics. Once per block is enough; the smoothers do the gliding.
    pub fn update_targets(&mut self) {
        for stem in 0..STEM_COUNT {
            self.target_gain[stem] = self.control.gain(stem);
            self.target_audible[stem] = if self.control.is_muted(stem) {
                0.0
            } else {
                1.0
            };
            self.filter[stem].set_position(self.control.filter(stem));
            self.filter[stem].refresh();
            let active = self.filter[stem].is_active();
            if active && !self.filter_on[stem] {
                // A filter that was bypassed has stale memory: start it from silence.
                self.filter[stem].reset();
            }
            self.filter_on[stem] = active;
        }
    }

    /// Mixes one source frame (`[drums L, drums R, bass L, bass R, vocals L, vocals R, other L,
    /// other R]`) into a stereo frame.
    #[inline]
    pub fn mix_frame(&mut self, frame: &[f32; STEM_FRAME_FLOATS]) -> [f32; 2] {
        let mut left = 0.0_f32;
        let mut right = 0.0_f32;
        for stem in 0..STEM_COUNT {
            let gain = settle(&mut self.gain[stem], self.target_gain[stem])
                * settle(&mut self.audible[stem], self.target_audible[stem]);
            let mut l = frame[stem * 2];
            let mut r = frame[stem * 2 + 1];
            if self.filter_on[stem] {
                l = self.filter[stem].process_sample(0, l);
                r = self.filter[stem].process_sample(1, r);
            }
            left += l * gain;
            right += r * gain;
        }
        [left, right]
    }

    /// Mixes a block: `src` holds eight floats per frame, and the shortest of the three slices
    /// sets the frame count. Reads the control state once, at the start.
    pub fn mix_block(&mut self, src: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        self.update_targets();
        for ((frame, l), r) in src
            .chunks_exact(STEM_FRAME_FLOATS)
            .zip(out_l.iter_mut())
            .zip(out_r.iter_mut())
        {
            let mut f = [0.0_f32; STEM_FRAME_FLOATS];
            f.copy_from_slice(frame);
            let [a, b] = self.mix_frame(&f);
            *l = a;
            *r = b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn mixer() -> (StemMixer, Arc<StemMixerControl>) {
        let control = Arc::new(StemMixerControl::new());
        let mut m = StemMixer::new(Arc::clone(&control));
        m.prepare(SR);
        (m, control)
    }

    fn test_frames(n: usize) -> Vec<f32> {
        (0..n * STEM_FRAME_FLOATS)
            .map(|i| ((i as f32) * 0.37).sin() * 0.2)
            .collect()
    }

    #[test]
    fn unity_gains_sum_the_four_stems() {
        let (mut m, _c) = mixer();
        let src = test_frames(1000);
        let (mut l, mut r) = (vec![0.0; 1000], vec![0.0; 1000]);
        m.mix_block(&src, &mut l, &mut r);
        for (i, frame) in src.chunks_exact(8).enumerate() {
            let el = frame[0] + frame[2] + frame[4] + frame[6];
            let er = frame[1] + frame[3] + frame[5] + frame[7];
            assert!(
                (l[i] - el).abs() < 1e-6 && (r[i] - er).abs() < 1e-6,
                "frame {i}"
            );
        }
    }

    #[test]
    fn mute_fades_without_a_click() {
        let (mut m, c) = mixer();
        let mut frame = [0.0_f32; 8];
        frame[0] = 1.0;
        frame[1] = 1.0;
        assert_eq!(m.mix_frame(&frame), [1.0, 1.0]);
        c.set_muted(0, true);
        m.update_targets();
        let mut prev = 1.0_f32;
        let mut last = 1.0_f32;
        for _ in 0..9600 {
            let [l, _] = m.mix_frame(&frame);
            assert!((prev - l).abs() < 0.01, "step {}", prev - l);
            assert!(l <= prev + 1e-7, "monotonic fade");
            prev = l;
            last = l;
        }
        assert_eq!(last, 0.0, "fully muted after the glide");
        c.set_muted(0, false);
        m.update_targets();
        let mut prev = 0.0_f32;
        for _ in 0..9600 {
            let [l, _] = m.mix_frame(&frame);
            assert!((l - prev).abs() < 0.01 && l >= prev - 1e-7);
            prev = l;
        }
        assert_eq!(prev, 1.0);
    }

    #[test]
    fn filter_at_zero_is_a_bit_exact_bypass() {
        let (mut m, c) = mixer();
        c.set_filter(1, 0.0);
        m.update_targets();
        let src = test_frames(500);
        let (mut l, mut r) = (vec![0.0; 500], vec![0.0; 500]);
        m.mix_block(&src, &mut l, &mut r);
        let (mut m2, _c2) = mixer();
        let (mut l2, mut r2) = (vec![0.0; 500], vec![0.0; 500]);
        m2.mix_block(&src, &mut l2, &mut r2);
        assert_eq!(l, l2);
        assert_eq!(r, r2);
        // And equal to the plain left-to-right sum, bit for bit.
        for (i, f) in src.chunks_exact(8).enumerate() {
            assert_eq!(l[i], f[0] + f[2] + f[4] + f[6]);
        }
    }

    #[test]
    fn a_low_pass_on_one_stem_removes_its_highs_only() {
        let (mut m, c) = mixer();
        c.set_filter(2, -0.5);
        m.update_targets();
        // Vocals (stem 2) carry a 10 kHz tone, drums a 100 Hz tone.
        let mut peak_hi = 0.0_f32;
        let mut peak_lo = 0.0_f32;
        for n in 0..8000 {
            let t = n as f32 / SR as f32;
            let mut f = [0.0_f32; 8];
            f[4] = (std::f32::consts::TAU * 10_000.0 * t).sin() * 0.5;
            f[5] = f[4];
            let [l, _] = m.mix_frame(&f);
            if n > 4000 {
                peak_hi = peak_hi.max(l.abs());
            }
        }
        let (mut m, c) = mixer();
        c.set_filter(2, -0.5);
        m.update_targets();
        for n in 0..8000 {
            let t = n as f32 / SR as f32;
            let mut f = [0.0_f32; 8];
            f[4] = (std::f32::consts::TAU * 100.0 * t).sin() * 0.5;
            f[5] = f[4];
            let [l, _] = m.mix_frame(&f);
            if n > 4000 {
                peak_lo = peak_lo.max(l.abs());
            }
        }
        assert!(peak_hi < 0.02, "highs removed, got {peak_hi}");
        assert!(peak_lo > 0.4, "lows kept, got {peak_lo}");
    }

    #[test]
    fn gain_step_response_is_monotonic_and_settles() {
        let (mut m, c) = mixer();
        c.set_gain(3, 0.25);
        m.update_targets();
        let mut f = [0.0_f32; 8];
        f[6] = 1.0;
        f[7] = 1.0;
        let mut prev = 1.0_f32;
        for _ in 0..9600 {
            let [l, _] = m.mix_frame(&f);
            assert!(l <= prev && l >= 0.25, "monotonic toward the target");
            prev = l;
        }
        assert_eq!(prev, 0.25);
    }

    #[test]
    fn gains_clamp_and_nan_is_ignored() {
        let c = StemMixerControl::new();
        c.set_gain(0, 9.0);
        assert_eq!(c.gain(0), MAX_STEM_GAIN);
        c.set_gain(0, -1.0);
        assert_eq!(c.gain(0), 0.0);
        c.set_gain(0, f32::NAN);
        assert_eq!(c.gain(0), 0.0);
        c.set_filter(1, 4.0);
        assert_eq!(c.filter(1), 1.0);
        c.set_gain(7, 0.5);
        assert_eq!(c.gain(7), 1.0);
    }

    #[test]
    fn filter_tail_never_goes_subnormal() {
        let (mut m, c) = mixer();
        c.set_filter(0, -0.8);
        m.update_targets();
        let mut f = [0.0_f32; 8];
        f[0] = 1.0;
        f[1] = 1.0;
        m.mix_frame(&f);
        f[0] = 0.0;
        f[1] = 0.0;
        for n in 0..300_000 {
            let [l, r] = m.mix_frame(&f);
            assert!(!l.is_subnormal() && !r.is_subnormal(), "frame {n}");
        }
    }

    #[test]
    fn control_is_send_and_sync() {
        // Control is Send + Sync so the engine can hand it to any control thread.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StemMixerControl>();
    }

    /// Cost of the 8-to-2 mix per 256-frame block. Run with
    /// `cargo test -p om-dsp --release stem_mix_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing measurement, not a correctness test"]
    fn stem_mix_cost() {
        fn per_block_us(filters: bool) -> f64 {
            let (mut m, c) = mixer();
            if filters {
                for stem in 0..4 {
                    c.set_filter(stem, [-0.6, 0.5, -0.3, 0.7][stem]);
                }
            }
            c.set_gain(1, 0.8);
            m.update_targets();
            let src = test_frames(256);
            let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
            let blocks = 20_000;
            let start = std::time::Instant::now();
            for _ in 0..blocks {
                m.mix_block(std::hint::black_box(&src), &mut l, &mut r);
                std::hint::black_box((&l, &r));
            }
            start.elapsed().as_secs_f64() * 1e6 / f64::from(blocks)
        }
        println!(
            "stem mix, no filters:  {:.2} us / 256-frame block",
            per_block_us(false)
        );
        println!(
            "stem mix, 4 filters:   {:.2} us / 256-frame block",
            per_block_us(true)
        );
    }
}
