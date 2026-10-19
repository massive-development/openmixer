//! Synthetic test signals for analysis tests (no copyrighted audio). Port of
//! `tests/unit/AnalysisTestSignals.h`, minus the WAV writer (no file tests yet).

use crate::AudioClip;
use std::f64::consts::PI;

pub const RATE: f64 = 44_100.0;

pub fn silence(seconds: f64, rate: f64) -> AudioClip {
    AudioClip::new(vec![0.0; (seconds * rate) as usize], rate)
}

/// Deterministic noise source standing in for `juce::Random`: same role, not the same sequence.
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

pub fn add_noise(clip: &mut AudioClip, gain: f32, seed: u64) {
    let mut random = TestRandom::new(seed);
    for s in &mut clip.samples {
        *s += gain * (2.0 * random.next_f32() - 1.0);
    }
}
