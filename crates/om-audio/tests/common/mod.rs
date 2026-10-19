//! Shared helpers for the om-audio test binaries: WAV fixtures in a per-process temp directory,
//! bounded polling, and level measures (peak and RMS).
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory under the system temp dir, removed on drop.
pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn new(prefix: &str) -> Self {
        let n = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path =
            std::env::temp_dir().join(format!("{prefix}_{}_{n}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Writes interleaved 32-bit float samples as a WAV file.
pub fn write_float_wav(path: &Path, channels: u16, rate: u32, samples: &[f32]) {
    let data_len = (samples.len() * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 4);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    let block_align = u32::from(channels) * 4;
    bytes.extend_from_slice(&(rate * block_align).to_le_bytes());
    bytes.extend_from_slice(&(block_align as u16).to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

/// Interleaved stereo sine at `amplitude`, `frames` long, identical on both channels.
pub fn sine_stereo(frequency: f64, amplitude: f64, frames: usize, rate: f64) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let phase = 2.0 * std::f64::consts::PI * frequency * i as f64 / rate;
        let v = (amplitude * phase.sin()) as f32;
        out.push(v);
        out.push(v);
    }
    out
}

/// Polls `cond` until it holds or `timeout` passes. Yields between checks.
pub fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let end = Instant::now() + timeout;
    while !cond() {
        if Instant::now() > end {
            return false;
        }
        std::thread::yield_now();
    }
    true
}

/// Largest absolute sample.
pub fn magnitude(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

/// Root mean square.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}
