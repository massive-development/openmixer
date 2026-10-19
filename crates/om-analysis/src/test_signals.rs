//! Synthetic test signals for analysis tests (no copyrighted audio)., plus a float WAV writer and a temporary-file helper for the
//! file-backed tests.

use crate::AudioClip;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const RATE: f64 = 44_100.0;

pub fn silence(seconds: f64, rate: f64) -> AudioClip {
    AudioClip::new(vec![0.0; (seconds * rate) as usize], rate)
}

/// Deterministic noise source; the sequence is not bit-compatible with any other generator.
/// The tests that use it are tolerance-based, so the exact noise does not matter.
pub struct TestRandom(u64);

impl TestRandom {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// splitmix64, mapped to `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        ((z >> 40) as f32) * (1.0 / 16_777_216.0)
    }
}

/// Damped 1 kHz burst (about 30 ms) at `time_seconds`.
pub fn add_click(clip: &mut AudioClip, time_seconds: f64, gain: f32) {
    add_damped_sine(clip, time_seconds, gain, 1000.0, 150.0, 0.03);
}

/// Kick-like low thump: damped 55 Hz sine, about 150 ms.
pub fn add_kick(clip: &mut AudioClip, time_seconds: f64, gain: f32) {
    add_damped_sine(clip, time_seconds, gain, 55.0, 25.0, 0.15);
}

fn add_damped_sine(
    clip: &mut AudioClip,
    time_seconds: f64,
    gain: f32,
    freq: f64,
    decay: f64,
    length_seconds: f64,
) {
    let rate = clip.sample_rate;
    let start = (time_seconds * rate) as usize;
    let length = (length_seconds * rate) as usize;
    for i in 0..length {
        let Some(sample) = clip.samples.get_mut(start + i) else {
            break;
        };
        let t = i as f64 / rate;
        *sample += gain * ((-t * decay).exp() * (2.0 * PI * freq * t).sin()) as f32;
    }
}

/// Click track. Every `beats_per_bar`-th beat, starting at beat `accent_beat`, also gets a kick
/// (negative `accent_beat` means no kicks).
pub fn click_track(
    bpm: f64,
    seconds: f64,
    first_beat_seconds: f64,
    accent_beat: i32,
    beats_per_bar: i32,
    rate: f64,
) -> AudioClip {
    let mut clip = silence(seconds, rate);
    let period = 60.0 / bpm;
    let mut beat = 0i32;
    let mut t = first_beat_seconds;
    while t < seconds {
        add_click(&mut clip, t, 0.8);
        if accent_beat >= 0 && beat % beats_per_bar == accent_beat {
            add_kick(&mut clip, t, 0.9);
        }
        t += period;
        beat += 1;
    }
    clip
}

/// Sine tone over the whole clip.
pub fn add_tone(clip: &mut AudioClip, freq: f64, gain: f32) {
    add_tone_between(clip, freq, gain, 0.0, f64::INFINITY);
}

/// Sine tone between `from_seconds` and `to_seconds` (clipped to the clip).
pub fn add_tone_between(
    clip: &mut AudioClip,
    freq: f64,
    gain: f32,
    from_seconds: f64,
    to_seconds: f64,
) {
    let rate = clip.sample_rate;
    let from = (from_seconds * rate) as usize;
    let to = ((to_seconds * rate) as usize).min(clip.samples.len());
    for i in from..to {
        let phase = 2.0 * PI * freq * i as f64 / rate;
        clip.samples[i] += gain * phase.sin() as f32;
    }
}

pub fn add_noise(clip: &mut AudioClip, gain: f32, seed: u64) {
    let mut random = TestRandom::new(seed);
    for s in &mut clip.samples {
        *s += gain * (2.0 * random.next_f32() - 1.0);
    }
}

/// Writes interleaved 32-bit float PCM (WAVE format 3).
pub fn write_float_wav(path: &Path, channels: u16, rate: u32, interleaved: &[f32]) {
    let data_len = (interleaved.len() * 4) as u32;
    let block_align = channels * 4;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in interleaved {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write test wav");
}

/// A unique path in the system temp directory, removed on drop.
pub struct TempFile(PathBuf);

impl TempFile {
    pub fn new(extension: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!(
            "om_analysis_{}_{}_{}.{extension}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            nanos
        );
        Self(std::env::temp_dir().join(name))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
