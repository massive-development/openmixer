#![forbid(unsafe_code)]

//! Filters, EQ, sweep filter, meters and crossfader curves. Samples are `f32`; process functions
//! work on caller-owned buffers and do not allocate.

mod biquad;
mod crossfader;
mod eq;
mod meter;
mod sweep;

pub use biquad::{
    design_high_pass, design_high_shelf, design_low_pass, design_low_shelf, design_peaking, Biquad,
    BiquadCoeffs,
};
pub use crossfader::{Crossfader, CrossfaderAssign, CrossfaderCurve, CrossfaderGains};
pub use eq::ThreeBandEq;
pub use meter::{MeterBallistics, MeterParams, PeakMeter};
pub use sweep::SweepFilter;

#[cfg(test)]
pub(crate) mod test_util {
    pub const SAMPLE_RATE: f64 = 48_000.0;
    pub const FRAMES: usize = 16_384;
    pub const BLOCK: usize = 512;

    /// Sine at `hz` with peak amplitude `amp`, computed in double then narrowed.
    pub fn sine(hz: f64, amp: f32) -> Vec<f32> {
        (0..FRAMES)
            .map(|i| {
                amp * ((2.0 * std::f64::consts::PI * hz * i as f64 / SAMPLE_RATE).sin() as f32)
            })
            .collect()
    }

    /// RMS over the second half of the buffer, after the filters have settled.
    pub fn rms_tail(samples: &[f32]) -> f64 {
        let tail = &samples[FRAMES / 2..];
        (tail
            .iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum::<f64>()
            / tail.len() as f64)
            .sqrt()
    }

    pub fn db_of(ratio: f64) -> f64 {
        20.0 * ratio.max(1e-12).log10()
    }

    /// Input RMS of a sine with peak `amp`.
    pub fn sine_rms(amp: f32) -> f64 {
        f64::from(amp) / 2.0_f64.sqrt()
    }
}
