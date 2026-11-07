//! A decoded sample: interleaved stereo `f32` at a known rate. Decoded off the audio thread.

use crate::deck::{SourceError, SourceReader};
use crate::resample::fft_resampler;
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::Resampler;
use std::path::Path;
use std::sync::Arc;

/// Longest sample accepted, in seconds of source audio.
pub const MAX_SAMPLE_SECONDS: f64 = 60.0;
/// Largest sample accepted, in bytes of decoded stereo `f32`.
pub const MAX_SAMPLE_BYTES: usize = 64 * 1024 * 1024;

const CHUNK_FRAMES: usize = 4096;

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
    #[error("resampling failed: {0}")]
    Resample(String),
    #[error("invalid sample rate {0}")]
    BadRate(f64),
    #[error("cannot read sample: {0}")]
    Source(#[from] SourceError),
    #[cfg(feature = "ffmpeg")]
    #[error("{0}")]
    Ffmpeg(#[from] crate::decode_ffmpeg::DecodeError),
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
        let mut reader = match SourceReader::open(path) {
            Ok(reader) => reader,
            #[cfg(feature = "ffmpeg")]
            Err(SourceError::Unsupported(why)) => {
                return Self::load_with_ffmpeg(path, engine_rate, why)
            }
            Err(e) => return Err(e.into()),
        };
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
            data = resample(&data, src_rate, engine_rate)?;
        }
        Self::from_interleaved(name, engine_rate, data)
    }

    /// Fallback for a format symphonia does not support (`why` is its error). Without a usable
    /// ffmpeg the original error is returned unchanged.
    #[cfg(feature = "ffmpeg")]
    fn load_with_ffmpeg(path: &Path, engine_rate: f64, why: String) -> Result<Self, SamplerError> {
        use crate::decode_ffmpeg::{decode_with_ffmpeg, DecodeError};
        // ffmpeg resamples to a whole rate; fractional engine rates are rounded.
        let rate = engine_rate.round() as u32;
        match decode_with_ffmpeg(path, rate) {
            Ok(audio) => {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("sample")
                    .to_owned();
                Self::from_interleaved(name, f64::from(audio.sample_rate), audio.samples)
            }
            Err(DecodeError::FfmpegMissing) => Err(SourceError::Unsupported(why).into()),
            Err(e) => Err(SamplerError::Ffmpeg(e)),
        }
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

/// Resamples interleaved stereo with rubato's synchronous FFT resampler (whole-Hz rates; a
/// fractional rate is rounded). Offline: runs on the control thread and allocates.
fn resample(input: &[f32], from: f64, to: f64) -> Result<Vec<f32>, SamplerError> {
    let frames_in = input.len() / 2;
    let fail = |e: &dyn std::fmt::Display| SamplerError::Resample(e.to_string());
    let mut resampler =
        fft_resampler(from.round() as usize, to.round() as usize, 2).map_err(|e| fail(&e))?;
    let source = InterleavedSlice::new(input, 2, frames_in).map_err(|e| fail(&e))?;
    let out = resampler
        .process_all(&source, frames_in, None)
        .map_err(|e| fail(&e))?;
    Ok(out.take_data())
}
