//! The synchronous core: an [`EbuR128`] meter fed interleaved stereo and read as one
//! [`LoudnessReading`]. The worker thread drives it, and it is usable offline on its own.

use super::{LoudnessError, LoudnessReading};
use ebur128::{EbuR128, Mode};

/// Channels the engine meters: the master bus, left and right.
pub const CHANNELS: u32 = 2;

/// Stereo EBU R128 meter: momentary, short-term, integrated, loudness range and true peak.
///
/// Integrated loudness and LRA use the crate's histogram mode, so memory stays bounded however long
/// the session runs (the queue mode keeps every gating block, which grows without limit and makes
/// each read walk the whole history).
pub struct LoudnessAnalyser {
    meter: EbuR128,
}

impl LoudnessAnalyser {
    /// `sample_rate` in Hz; the crate accepts 16 Hz to 2.8 MHz. Allocates, so not for the audio
    /// thread.
    pub fn new(sample_rate: u32) -> Result<Self, LoudnessError> {
        let mode = Mode::M | Mode::S | Mode::I | Mode::LRA | Mode::TRUE_PEAK | Mode::HISTOGRAM;
        let meter = EbuR128::new(CHANNELS, sample_rate, mode)
            .map_err(|e| LoudnessError::Meter(e.to_string()))?;
        Ok(Self { meter })
    }

    pub fn sample_rate(&self) -> u32 {
        self.meter.rate()
    }

    /// Adds interleaved L/R samples (an odd trailing sample is ignored).
    pub fn add_interleaved(&mut self, samples: &[f32]) {
        let whole = samples.len() & !1;
        // Only fails on a mode or channel mismatch, which `new` rules out.
        let _ = self.meter.add_frames_f32(&samples[..whole]);
    }

    /// Forgets everything measured so far (integrated, LRA, true-peak maximum, filter state).
    pub fn reset(&mut self) {
        self.meter.reset();
    }

    /// The current values. `-inf` LUFS means "nothing above the gate yet".
    pub fn reading(&self) -> LoudnessReading {
        let lufs = |r: Result<f64, ebur128::Error>| r.map_or(f32::NEG_INFINITY, |v| v as f32);
        let peak = |channel: u32| {
            self.meter
                .true_peak(channel)
                .map_or(f32::NEG_INFINITY, |linear| linear_to_db(linear) as f32)
        };
        LoudnessReading {
            momentary_lufs: lufs(self.meter.loudness_momentary()),
            short_term_lufs: lufs(self.meter.loudness_shortterm()),
            integrated_lufs: lufs(self.meter.loudness_global()),
            loudness_range_lu: self.meter.loudness_range().map_or(0.0, |v| v as f32),
            true_peak_dbtp: [peak(0), peak(1)],
            ..LoudnessReading::SILENT
        }
    }
}

fn linear_to_db(linear: f64) -> f64 {
    if linear > 0.0 {
        20.0 * linear.log10()
    } else {
        f64::NEG_INFINITY
    }
}
