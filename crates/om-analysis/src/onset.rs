//! Onset strength over time: positive log-spectral flux within frequency bands, then the local mean
//! removed and half-wave rectified. Shared by tempo, downbeat and structure analysis. Port of
//! `src/analysis/OnsetEnvelope.{h,cpp}`.

use crate::{AnalysisContext, AudioClip, MagnitudeStft};

const LOG_GAIN: f32 = 1000.0;
/// Flux peaks about 2 ms before the onset (calibrated on clicks).
const PEAK_LEAD_SECONDS: f64 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrequencyBand {
    pub low_hz: f64,
    pub high_hz: f64,
}

impl Default for FrequencyBand {
    fn default() -> Self {
        Self {
            low_hz: 30.0,
            high_hz: 16_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnsetOptions {
    /// 1024-sample frames by default.
    pub fft_order: u32,
    pub hop: usize,
    /// Zero keeps the raw flux.
    pub mean_window_seconds: f64,
}

impl Default for OnsetOptions {
    fn default() -> Self {
        Self {
            fft_order: 10,
            hop: 512,
            mean_window_seconds: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OnsetEnvelope {
    pub values: Vec<f32>,
    /// Values per second.
    pub frame_rate: f64,
    /// Time of `values[0]`.
    pub start_seconds: f64,
}

impl OnsetEnvelope {
    pub fn time_of(&self, index: f64) -> f64 {
        self.start_seconds + index / self.frame_rate
    }

    pub fn index_of(&self, seconds: f64) -> f64 {
        (seconds - self.start_seconds) * self.frame_rate
    }

    /// Linear interpolation; zero outside the envelope.
    pub fn at(&self, index: f64) -> f32 {
        if index < 0.0 || self.values.is_empty() {
            return 0.0;
        }
        let i = index as usize;
        if i + 1 >= self.values.len() {
            return self.values.get(i).copied().unwrap_or(0.0);
        }
        let frac = (index - i as f64) as f32;
        self.values[i] + frac * (self.values[i + 1] - self.values[i])
    }
}

/// Inclusive bin range of a band.
#[derive(Clone, Copy)]
struct BinRange {
    first: usize,
    last: usize,
}

fn bins_of(stft: &MagnitudeStft, band: FrequencyBand, sample_rate: f64) -> BinRange {
    let bin_hz = stft.bin_hz(1, sample_rate);
    let top = (stft.num_bins() - 1) as i64;
    let first = (((band.low_hz / bin_hz).ceil()) as i64).max(1).min(top);
    let last = (((band.high_hz / bin_hz).floor()) as i64)
        .max(first)
        .min(top);
    BinRange {
        first: first as usize,
        last: last as usize,
    }
}

fn positive_flux(now: &[f32], previous: &[f32], bins: BinRange) -> f32 {
    let mut flux = 0.0f32;
    for k in bins.first..=bins.last {
        flux += (now[k] - previous[k]).max(0.0);
    }
    flux
}

fn empty_envelope(stft: &MagnitudeStft, sample_rate: f64) -> OnsetEnvelope {
    OnsetEnvelope {
        values: Vec::new(),
        frame_rate: sample_rate / stft.hop() as f64,
        // Frame centre, plus the flux peak lead.
        start_seconds: 0.5 * stft.frame_size() as f64 / sample_rate + PEAK_LEAD_SECONDS,
    }
}

/// Subtracts the centred moving mean (window `2 * radius + 1`, clipped at the ends) and rectifies.
fn remove_local_mean(values: &mut Vec<f32>, radius: i64) {
    if radius <= 0 || values.is_empty() {
        return;
    }
    let mut prefix = vec![0.0f64; values.len() + 1];
    for (i, &v) in values.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(v);
    }

    let n = values.len() as i64;
    let out: Vec<f32> = (0..n)
        .map(|i| {
            let lo = (i - radius).max(0) as usize;
            let hi = (i + radius + 1).min(n) as usize;
            let mean = (prefix[hi] - prefix[lo]) / (hi - lo) as f64;
            (values[i as usize] - mean as f32).max(0.0)
        })
        .collect();
    *values = out;
}

/// One envelope per band from a single STFT pass; `None` when cancelled. Reports progress.
pub fn compute_band_onsets(
    clip: &AudioClip,
    options: &OnsetOptions,
    bands: &[FrequencyBand],
    context: &dyn AnalysisContext,
) -> Option<Vec<OnsetEnvelope>> {
    let mut stft = MagnitudeStft::new(options.fft_order, options.hop);
    let frames = stft.num_frames(clip.samples.len());
    let ranges: Vec<BinRange> = bands
        .iter()
        .map(|&band| bins_of(&stft, band, clip.sample_rate))
        .collect();
    let mut envelopes: Vec<OnsetEnvelope> = bands
        .iter()
        .map(|_| empty_envelope(&stft, clip.sample_rate))
        .collect();
    for env in &mut envelopes {
        env.values.reserve(frames);
    }

    let mut magnitudes = Vec::new();
    let mut levels = Vec::new();
    let mut previous = vec![0.0f32; stft.num_bins()];
    for i in 0..frames {
        if i % 256 == 0 && context.is_cancelled() {
            return None;
        }
        stft.compute_frame(&clip.samples, i, &mut magnitudes);
        levels.clear();
        levels.extend(magnitudes.iter().map(|&m| (LOG_GAIN * m).ln_1p()));
        for (env, &range) in envelopes.iter_mut().zip(&ranges) {
            env.values.push(positive_flux(&levels, &previous, range));
        }
        std::mem::swap(&mut previous, &mut levels);
        context.set_progress((i + 1) as f32 / frames as f32);
    }

    for env in &mut envelopes {
        let radius = (options.mean_window_seconds * env.frame_rate * 0.5) as i64;
        remove_local_mean(&mut env.values, radius);
    }
    Some(envelopes)
}
