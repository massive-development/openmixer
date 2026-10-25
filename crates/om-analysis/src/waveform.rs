//! Builds [`WaveformData`] from a clip: the peak range of each block of `samples_per_point`
//! samples, plus loudness and the peak of the low, mid and high band from an overlapping FFT. FFT
//! frames are `hop` apart and each point blends the two frames around its centre, so the band
//! balance changes smoothly along the strip. A band's peak is the point's peak times the square
//! root of the band's energy share. Stateless and thread-safe. [`WaveformAnalyzer`] wraps it for the job service
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

/// What one FFT frame says about the audio around it.
#[derive(Debug, Clone, Copy)]
struct Frame {
    moment: Moment,
    /// Energy share of the low, mid and high band.
    bands: [f32; 3],
}

/// Loudness (mean square) and band shares of the blend of the frames around `centre` (a sample
/// index), weighted by closeness. The band shares blend by energy, so a loud frame outweighs a
/// quiet neighbour.
fn blend_frames(frames: &[Frame], centre: f64, frame_size: usize, hop: usize) -> (f32, [f32; 3]) {
    let position = ((centre - frame_size as f64 / 2.0) / hop as f64).max(0.0);
    let last = frames.len() - 1;
    let first = (position.floor() as usize).min(last);
    let second = (first + 1).min(last);
    let into = (position - first as f64).clamp(0.0, 1.0) as f32;
    let parts = [(1.0 - into, frames[first]), (into, frames[second])];
    let energy = pool(parts.map(|(w, f)| (w, f.moment))).energy;
    let mut bands = [0.0f32; 3];
    for (w, f) in parts {
        for (b, share) in bands.iter_mut().zip(f.bands) {
            *b += w * f.moment.energy * share;
        }
    }
    let total: f32 = bands.iter().sum();
    if total > 0.0 {
        bands = bands.map(|b| b / total);
    }
    (energy, bands)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformBuilder {
    options: WaveformOptions,
}

impl WaveformBuilder {
    pub fn new(options: WaveformOptions) -> Self {
        Self { options }
    }

    fn analyse_frames(
        &self,
        clip: &AudioClip,
        context: &dyn AnalysisContext,
    ) -> Option<(FrameAnalyzer, Vec<Frame>)> {
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
            let moment = analyzer.frame(&clip.samples, index);
            let bands = analyzer.frame_bands(&clip.samples, index);
            frames.push(Frame { moment, bands });
        }
        Some((analyzer, frames))
    }

    /// `None` when cancelled; invalid (empty) data for an empty clip.
    pub fn build(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<WaveformData> {
        let total = clip.samples.len();
        let block = self.options.samples_per_point.max(1);
        let (analyzer, frames) = self.analyse_frames(clip, context)?;
        let mut base = WaveformLevel {
            samples_per_point: block,
            points: Vec::with_capacity(total / block + 1),
        };
        for (index, chunk) in clip.samples.chunks(block).enumerate() {
            if index % 1024 == 0 && context.is_cancelled() {
                return None;
            }
            let centre = (index * block) as f64 + chunk.len() as f64 / 2.0;
            let (energy, shares) =
                blend_frames(&frames, centre, analyzer.frame_size(), analyzer.hop());
            let (min, max) = peak_range(chunk);
            let peak = min.abs().max(max.abs()).min(1.0);
            let [low, mid, high] = shares.map(|share| peak * share.sqrt());
            base.points.push(WaveformPoint {
                min,
                max,
                loudness: energy.max(0.0).sqrt().min(1.0),
                low,
                mid,
                high,
            });
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

    /// 3: band peak points (format 3); version 2 and 1 entries read as missing (the library still
    /// draws them until this runs).
    fn version(&self) -> i32 {
        3
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
