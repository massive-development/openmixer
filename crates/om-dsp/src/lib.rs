#![forbid(unsafe_code)]

//! Filters, EQ, sweep filter, meters and crossfader curves. Samples are `f32`; process functions
//! work on caller-owned buffers and do not allocate.

mod biquad;
mod crossfader;
mod eq;
mod meter;
mod smoother;
mod sweep;

pub use biquad::{
    design_high_pass, design_high_shelf, design_low_pass, design_low_shelf, design_peaking, Biquad,
    BiquadCoeffs,
};
pub use crossfader::{Crossfader, CrossfaderAssign, CrossfaderCurve, CrossfaderGains};
pub use eq::ThreeBandEq;
pub use meter::{MeterBallistics, MeterParams, PeakMeter};
pub use smoother::Smoother;
pub use sweep::SweepFilter;

#[cfg(test)]
pub(crate) mod test_util {
    pub const SAMPLE_RATE: f64 = 48_000.0;
    pub const FRAMES: usize = 16_384;
    pub const BLOCK: usize = 512;
}
