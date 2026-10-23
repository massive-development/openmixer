//! Deterministic test signals. Nothing here is random across runs: noise comes from a fixed-seed
//! LCG, so a failing test reproduces exactly.

use std::f64::consts::TAU;

/// Sine at `freq` Hz with peak amplitude 1.0, `frames` long.
pub fn sine(freq: f64, sample_rate: f64, frames: usize) -> Vec<f32> {
    sine_amp(freq, 1.0, sample_rate, frames)
}

/// Sine at `freq` Hz with peak amplitude `amp`. Computed in double, then narrowed.
pub fn sine_amp(freq: f64, amp: f32, sample_rate: f64, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|i| (f64::from(amp) * (TAU * freq * i as f64 / sample_rate).sin()) as f32)
        .collect()
}

/// Interleaved stereo sine with identical channels and peak amplitude `amp`.
pub fn stereo_sine(freq: f64, amp: f32, sample_rate: f64, frames: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    for s in sine_amp(freq, amp, sample_rate, frames) {
        out.push(s);
        out.push(s);
    }
    out
}

/// Uniform white noise in `[-1, 1)` from a 64-bit LCG (Knuth's MMIX constants).
pub fn noise(seed: u64, n: usize) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // Top 24 bits give an exact f32 mantissa.
            let unit = (state >> 40) as f32 / 16_777_216.0;
            unit * 2.0 - 1.0
        })
        .collect()
}

/// A single impulse of height `amp` at frame 0, then zeros, `frames` long.
pub fn impulse(amp: f32, frames: usize) -> Vec<f32> {
    impulse_train(0, amp, frames)
}

/// Impulses of height `amp` every `period` frames, starting at frame 0. A `period` of 0 yields
/// silence.
pub fn impulse_train(period: usize, amp: f32, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames];
    if period > 0 {
        for v in out.iter_mut().step_by(period) {
            *v = amp;
        }
    }
    out
}

/// Which parts a [`mix_fixture`] plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixStyle {
    /// Kick, bass, snare, hats, vocal and pad, in sections.
    Full,
    /// Sub bass and kick carry the track; a few hats.
    HeavyBass,
    /// A vocal-like harmonic line over a pad and soft drums.
    VocalLed,
    /// Pad and sparse hats; long gaps.
    Sparse,
}

/// A deterministic 120 BPM mono track built from synthetic drums, bass, a vocal-like voice and a
/// pad, for tests that need music-like spectra without audio files. Peaks stay below 1.0.
pub fn mix_fixture(style: MixStyle, sample_rate: f64, seconds: f64) -> Vec<f32> {
    let frames = (seconds * sample_rate) as usize;
    let noise_line = noise(7, frames);
    let beat = (0.5 * sample_rate) as usize;
    let (mut out, mut hat_state) = (vec![0.0f32; frames], 0.0f32);
    for (i, sample) in out.iter_mut().enumerate() {
        let t = i as f64 / sample_rate;
        let (bar, in_beat) = (i / (4 * beat), (i % beat) as f64 / sample_rate);
        let step = i / (beat / 2) % 8;
        let section = (bar / 4) % 4;
        let hat_hp = noise_line[i] - hat_state;
        hat_state = noise_line[i];
        let (kick, bass, snare, hat, voice, pad) = match style {
            MixStyle::Full => (1.0, 0.7, 1.0, 1.0, 1.0, 0.6),
            MixStyle::HeavyBass => (1.0, 1.5, 0.0, 0.3, 0.0, 0.0),
            MixStyle::VocalLed => (0.4, 0.3, 0.5, 0.4, 1.4, 0.8),
            MixStyle::Sparse => (0.0, 0.0, 0.0, 0.5, 0.0, 1.0),
        };
        let beat_in_bar = i / beat % 4;
        let on = |from: usize| section >= from;
        let mut v = 0.0f64;
        if on(1) {
            v += f64::from(kick)
                * 0.5
                * (-in_beat * 18.0).exp()
                * (TAU * (50.0 + 70.0 * (-in_beat * 40.0).exp()) * in_beat).sin();
            v += f64::from(bass)
                * 0.18
                * (0..4)
                    .map(|h| (TAU * 55.0 * f64::from(1 + h) * t).sin() / f64::from(1 + h))
                    .sum::<f64>()
                * (1.0 - in_beat * 1.2).max(0.0);
        }
        if on(1) && beat_in_bar % 2 == 1 {
            v += f64::from(snare)
                * 0.25
                * (-in_beat * 25.0).exp()
                * (f64::from(noise_line[i]) * 0.7 + (TAU * 190.0 * t).sin() * 0.5);
        }
        if section != 3 || step % 2 == 0 {
            let open = if step % 4 == 2 { 6.0 } else { 20.0 };
            let sub = (i % (beat / 2)) as f64 / sample_rate;
            v += f64::from(hat) * 0.2 * (-sub * open * 4.0).exp() * f64::from(hat_hp);
        }
        if section == 2 || (section == 1 && bar % 2 == 0) {
            let f0 = 220.0 * (1.0 + 0.01 * (TAU * 5.5 * t).sin());
            let line: f64 = [
                (1.0, 0.6),
                (2.0, 1.0),
                (3.0, 0.8),
                (5.0, 0.5),
                (8.0, 0.3),
                (12.0, 0.2),
            ]
            .iter()
            .map(|(h, a)| a * (TAU * f0 * h * t).sin())
            .sum();
            v += f64::from(voice) * 0.12 * line * (0.5 + 0.5 * (TAU * 0.5 * t).sin());
        }
        v += f64::from(pad)
            * 0.06
            * [261.6, 329.6, 392.0]
                .iter()
                .map(|f| (TAU * f * t).sin())
                .sum::<f64>();
        *sample = (v * 0.6) as f32;
    }
    out
}
