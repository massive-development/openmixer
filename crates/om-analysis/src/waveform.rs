//! Builds [`WaveformData`] from a clip: the peak range of each block of `samples_per_point`
//! samples, plus loudness, spectral centroid and spread from an overlapping FFT. FFT frames are
//! `hop` apart and each point blends the two frames around its centre, so colour changes smoothly
//! along the strip. Stateless and thread-safe. [`WaveformAnalyzer`] wraps it for the job service
//! (id "waveform").

use crate::spectrum::{pool, FrameAnalyzer, Moment};
use crate::{AnalysisContext, Analyzer, AudioClip, WaveformData, WaveformLevel, WaveformPoint};

/// Frames between cancellation checks and progress reports.
const REPORT_EVERY: usize = 256;
/// Share of the progress bar the FFT pass takes; assembling the points is the rest.
const FFT_PROGRESS: f32 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformOptions {
    /// Finest level: about 2.9 ms at 44.1 kHz.
    pub samples_per_point: usize,
    /// FFT frame of `1 << fft_order` samples (2048: 46 ms, bins of 21.5 Hz at 44.1 kHz).
    pub fft_order: u32,
    /// Samples between FFT frames.
    pub hop: usize,
}

impl Default for WaveformOptions {
    fn default() -> Self {
        Self {
            samples_per_point: 128,
            fft_order: 11,
            hop: 512,
        }
    }
}

fn peak_range(block: &[f32]) -> (f32, f32) {
    block
        .iter()
        .fold((1.0f32, -1.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)))
}

/// The blend of the frames around `centre` (a sample index), weighted by closeness.
fn blend_frames(frames: &[Moment], centre: f64, frame_size: usize, hop: usize) -> Moment {
    let position = ((centre - frame_size as f64 / 2.0) / hop as f64).max(0.0);
    let last = frames.len() - 1;
    let first = (position.floor() as usize).min(last);
    let second = (first + 1).min(last);
    let into = (position - first as f64).clamp(0.0, 1.0) as f32;
    pool([(1.0 - into, frames[first]), (into, frames[second])])
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformBuilder {
    options: WaveformOptions,
}

impl WaveformBuilder {
    pub fn new(options: WaveformOptions) -> Self {
        Self { options }
    }

    fn frame_moments(
        &self,
        clip: &AudioClip,
        context: &dyn AnalysisContext,
    ) -> Option<(FrameAnalyzer, Vec<Moment>)> {
        let (order, hop) = (self.options.fft_order, self.options.hop);
        let mut analyzer = FrameAnalyzer::new(order, hop, clip.sample_rate);
        let count = analyzer.num_frames(clip.samples.len());
        let mut frames = Vec::with_capacity(count);
        for index in 0..count {
            if index % REPORT_EVERY == 0 {
                if context.is_cancelled() {
                    return None;
                }
                context.set_progress(FFT_PROGRESS * index as f32 / count as f32);
            }
            frames.push(analyzer.frame(&clip.samples, index));
        }
        Some((analyzer, frames))
    }

    /// `None` when cancelled; invalid (empty) data for an empty clip.
    pub fn build(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<WaveformData> {
        let total = clip.samples.len();
        let block = self.options.samples_per_point.max(1);
        let (analyzer, frames) = self.frame_moments(clip, context)?;
        let mut base = WaveformLevel {
            samples_per_point: block,
            points: Vec::with_capacity(total / block + 1),
        };
        for (index, chunk) in clip.samples.chunks(block).enumerate() {
            if index % 1024 == 0 && context.is_cancelled() {
                return None;
            }
            let centre = (index * block) as f64 + chunk.len() as f64 / 2.0;
            let moment = blend_frames(&frames, centre, analyzer.frame_size(), analyzer.hop());
            let (min, max) = peak_range(chunk);
            base.points
                .push(WaveformPoint::from_moment(min, max, moment));
        }
        context.set_progress(1.0);
        Some(WaveformData::new(clip.sample_rate, total as i64, base))
    }
}

/// [`Analyzer`] wrapper for the job service (id "waveform").
#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformAnalyzer {
    builder: WaveformBuilder,
}

impl Analyzer for WaveformAnalyzer {
    fn id(&self) -> &str {
        "waveform"
    }

    /// 2: spectral points (format 2); version 1 entries read as missing.
    fn version(&self) -> i32 {
        2
    }

    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        let data = self.builder.build(clip, context)?;
        if context.is_cancelled() {
            return None;
        }
        Some(data.to_json())
    }
}

#[cfg(test)]
mod legacy_reference;
#[cfg(test)]
mod tests;
