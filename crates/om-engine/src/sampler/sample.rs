//! A decoded sample: interleaved stereo `f32` at a known rate. Decoded off the audio thread.

use crate::deck::{SourceError, SourceReader};
use std::path::Path;
use std::sync::Arc;

/// Longest sample accepted, in seconds of source audio.
pub const MAX_SAMPLE_SECONDS: f64 = 60.0;
/// Largest sample accepted, in bytes of decoded stereo `f32`.
pub const MAX_SAMPLE_BYTES: usize = 64 * 1024 * 1024;

const CHUNK_FRAMES: usize = 4096;
/// Zero crossings each side of the resampling kernel.
const SINC_ZERO_CROSSINGS: f64 = 12.0;

#[derive(Debug, thiserror::Error)]
pub enum SamplerError {
    #[error("pad {0} does not exist")]
    BadPad(u8),
    #[error("the sample has no audio")]
    Empty,
    #[error("the sample is too long or too large (limit 60 s / 64 MiB)")]
    TooLarge,
    #[error("the file is truncated or corrupt: {0}")]
    Corrupt(String),
    #[error("invalid sample rate {0}")]
    BadRate(f64),
    #[error("cannot read sample: {0}")]
    Source(#[from] SourceError),
    #[error("the sampler command queue is full")]
    QueueFull,
}

/// Immutable audio shared between the control side, the pads and the voices.
#[derive(Debug)]
pub struct Sample {
    data: Arc<[f32]>,
    sample_rate: f64,
    name: String,
}

impl Sample {
    /// From interleaved stereo frames (`[l, r, l, r, ...]`) at `sample_rate`. Non-finite values
    /// become silence and denormals are flushed.
    pub fn from_interleaved(
        name: impl Into<String>,
        sample_rate: f64,
        mut data: Vec<f32>,
    ) -> Result<Self, SamplerError> {
        if !(sample_rate.is_finite() && sample_rate >= 1.0) {
            return Err(SamplerError::BadRate(sample_rate));
        }
        if !data.len().is_multiple_of(2) {
            data.pop();
        }
        if data.is_empty() {
            return Err(SamplerError::Empty);
        }
        if data.len() * 4 > MAX_SAMPLE_BYTES
            || data.len() as f64 / 2.0 / sample_rate > MAX_SAMPLE_SECONDS
        {
            return Err(SamplerError::TooLarge);
        }
        for v in &mut data {
            if !v.is_finite() || v.abs() < 1.0e-20 {
                *v = 0.0;
            }
        }
        Ok(Self {
            data: data.into(),
            sample_rate,
            name: name.into(),
        })
    }

    /// From mono frames, duplicated to both sides.
    pub fn from_mono(
        name: impl Into<String>,
        sample_rate: f64,
        mono: &[f32],
    ) -> Result<Self, SamplerError> {
        let data: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        Self::from_interleaved(name, sample_rate, data)
    }

    /// Decodes `path` with symphonia, mixes mono to stereo and resamples to `engine_rate`. Control
    /// thread (it reads a file and allocates). Rejects more than [`MAX_SAMPLE_SECONDS`] of source
    /// audio or [`MAX_SAMPLE_BYTES`] of decoded data.
    pub fn load(path: &Path, engine_rate: f64) -> Result<Self, SamplerError> {
        if !(engine_rate.is_finite() && engine_rate >= 1.0) {
            return Err(SamplerError::BadRate(engine_rate));
        }
        let mut reader = SourceReader::open(path)?;
        let info = reader.info();
        let src_rate = info.sample_rate;
        let max_frames = ((MAX_SAMPLE_SECONDS * src_rate) as usize).min(MAX_SAMPLE_BYTES / 8);
        // A container that does not state its length (or states 0, as some streamed or VBR files
        // do) is decoded to the end under the size cap.
        let declared = usize::try_from(info.length_frames).unwrap_or(0);
        if declared > max_frames {
            return Err(SamplerError::TooLarge);
        }
        let mut data = decode_all(&mut reader, declared, max_frames)?;
        if data.is_empty() {
            return Err(SamplerError::Empty);
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("sample")
            .to_owned();
        if (src_rate - engine_rate).abs() > 1.0e-6 {
            data = resample(&data, src_rate, engine_rate);
        }
        Self::from_interleaved(name, engine_rate, data)
    }

    /// Interleaved stereo data.
    pub fn data(&self) -> &Arc<[f32]> {
        &self.data
    }

    pub fn frames(&self) -> usize {
        self.data.len() / 2
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Decodes `reader` to its end as interleaved stereo. `declared` is the frame count the container
/// stated (0 when unknown, then only the size cap bounds the read). A decode failure or a file
/// holding clearly less than it declared is an error, never a silently shorter sample.
pub(super) fn decode_all(
    reader: &mut SourceReader,
    declared: usize,
    max_frames: usize,
) -> Result<Vec<f32>, SamplerError> {
    let mut data: Vec<f32> = Vec::with_capacity(declared * 2);
    let mut chunk = vec![[0.0_f32; 2]; CHUNK_FRAMES];
    let mut pos = 0_i64;
    loop {
        let got = reader.read_stereo(pos, &mut chunk);
        for frame in &chunk[..got] {
            data.extend_from_slice(frame);
        }
        pos += got as i64;
        if data.len() / 2 > max_frames {
            return Err(SamplerError::TooLarge);
        }
        if got < CHUNK_FRAMES {
            break;
        }
    }
    if reader.damaged() {
        return Err(SamplerError::Corrupt(format!(
            "decoding failed after {} frames",
            data.len() / 2
        )));
    }
    // The header promised more audio than the file holds (a cut-off download).
    if declared > 0 && (data.len() / 2) * 100 < declared * 99 {
        return Err(SamplerError::Corrupt(format!(
            "{} of {declared} frames present",
            data.len() / 2
        )));
    }
    Ok(data)
}

/// Windowed-sinc resampling of interleaved stereo (Blackman window, band-limited when
/// downsampling). Offline: runs on the control thread.
fn resample(input: &[f32], from: f64, to: f64) -> Vec<f32> {
    let frames_in = input.len() / 2;
    let out_frames = ((frames_in as f64) * to / from).round() as usize;
    let step = from / to;
    let cutoff = (to / from).min(1.0);
    let support = SINC_ZERO_CROSSINGS / cutoff;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let center = i as f64 * step;
        let first = (center - support).ceil().max(0.0) as usize;
        let last = ((center + support).floor() as usize).min(frames_in.saturating_sub(1));
        let (mut acc_l, mut acc_r, mut norm) = (0.0_f64, 0.0_f64, 0.0_f64);
        for k in first..=last {
            let x = k as f64 - center;
            let w = kernel(x, cutoff, support);
            acc_l += w * f64::from(input[k * 2]);
            acc_r += w * f64::from(input[k * 2 + 1]);
            norm += w;
        }
        if norm.abs() > 1.0e-9 {
            acc_l /= norm;
            acc_r /= norm;
        }
        out.push(acc_l as f32);
        out.push(acc_r as f32);
    }
    out
}

fn kernel(x: f64, cutoff: f64, support: f64) -> f64 {
    let sinc = if x.abs() < 1.0e-12 {
        1.0
    } else {
        let a = std::f64::consts::PI * cutoff * x;
        a.sin() / a
    };
    let t = (x / support).clamp(-1.0, 1.0);
    let window = 0.42
        + 0.5 * (std::f64::consts::PI * t).cos()
        + 0.08 * (2.0 * std::f64::consts::PI * t).cos();
    cutoff * sinc * window
}
