//! A deterministic 120 BPM mono track built from synthetic drums, bass, a vocal-like voice and a
//! pad, for tests that need music-like spectra without audio files. Peaks stay below 1.0.

use crate::signals::noise;
use std::f64::consts::TAU;

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

/// Level of each part, as a gain.
struct Levels {
    kick: f64,
    bass: f64,
    snare: f64,
    hat: f64,
    voice: f64,
    pad: f64,
}

impl Levels {
    fn of(style: MixStyle) -> Self {
        let (kick, bass, snare, hat, voice, pad) = match style {
            MixStyle::Full => (1.0, 0.7, 1.0, 1.0, 1.0, 0.6),
            MixStyle::HeavyBass => (1.0, 1.5, 0.0, 0.3, 0.0, 0.0),
            MixStyle::VocalLed => (0.4, 0.3, 0.5, 0.4, 1.4, 0.8),
            MixStyle::Sparse => (0.0, 0.0, 0.0, 0.5, 0.0, 1.0),
        };
        Self {
            kick,
            bass,
            snare,
            hat,
            voice,
            pad,
        }
    }
}

/// Where sample `i` falls in the song.
struct Position {
    t: f64,
    /// Seconds since the last beat.
    in_beat: f64,
    /// Seconds since the last eighth note.
    in_step: f64,
    beat_in_bar: usize,
    step: usize,
    bar: usize,
    /// Four-bar section, cycling through four.
    section: usize,
}

impl Position {
    fn at(i: usize, rate: f64) -> Self {
        let beat = (0.5 * rate) as usize;
        let bar = i / (4 * beat);
        Self {
            t: i as f64 / rate,
            in_beat: (i % beat) as f64 / rate,
            in_step: (i % (beat / 2)) as f64 / rate,
            beat_in_bar: i / beat % 4,
            step: i / (beat / 2) % 8,
            bar,
            section: (bar / 4) % 4,
        }
    }
}

fn sine(hz: f64, t: f64) -> f64 {
    (TAU * hz * t).sin()
}

fn kick(p: &Position) -> f64 {
    let sweep = 50.0 + 70.0 * (-p.in_beat * 40.0).exp();
    0.5 * (-p.in_beat * 18.0).exp() * (TAU * sweep * p.in_beat).sin()
}

fn bass(p: &Position) -> f64 {
    let stack: f64 = (1..=4)
        .map(|h| sine(55.0 * f64::from(h), p.t) / f64::from(h))
        .sum();
    0.18 * stack * (1.0 - p.in_beat * 1.2).max(0.0)
}

fn snare(p: &Position, hiss: f64) -> f64 {
    0.25 * (-p.in_beat * 25.0).exp() * (hiss * 0.7 + sine(190.0, p.t) * 0.5)
}

fn hat(p: &Position, hiss: f64) -> f64 {
    let decay = if p.step % 4 == 2 { 6.0 } else { 20.0 };
    0.2 * (-p.in_step * decay * 4.0).exp() * hiss
}

fn voice(p: &Position) -> f64 {
    let f0 = 220.0 * (1.0 + 0.01 * sine(5.5, p.t));
    let formants = [
        (1.0, 0.6),
        (2.0, 1.0),
        (3.0, 0.8),
        (5.0, 0.5),
        (8.0, 0.3),
        (12.0, 0.2),
    ];
    let line: f64 = formants.iter().map(|(h, a)| a * sine(f0 * h, p.t)).sum();
    0.12 * line * (0.5 + 0.5 * sine(0.5, p.t))
}

fn pad(p: &Position) -> f64 {
    0.06 * [261.6, 329.6, 392.0]
        .iter()
        .map(|f| sine(*f, p.t))
        .sum::<f64>()
}

fn sample(l: &Levels, p: &Position, hiss: f64) -> f64 {
    let mut v = 0.0;
    if p.section >= 1 {
        v += l.kick * kick(p) + l.bass * bass(p);
        if p.beat_in_bar % 2 == 1 {
            v += l.snare * snare(p, hiss);
        }
    }
    if p.section != 3 || p.step.is_multiple_of(2) {
        v += l.hat * hat(p, hiss);
    }
    if p.section == 2 || (p.section == 1 && p.bar.is_multiple_of(2)) {
        v += l.voice * voice(p);
    }
    v + l.pad * pad(p)
}

/// `seconds` of the track at `sample_rate`; the same arguments always give the same samples.
pub fn mix_fixture(style: MixStyle, sample_rate: f64, seconds: f64) -> Vec<f32> {
    let frames = (seconds * sample_rate) as usize;
    let (levels, noise_line) = (Levels::of(style), noise(7, frames));
    let mut previous = 0.0f32;
    (0..frames)
        .map(|i| {
            // First difference of white noise: the hats' bright hiss.
            let hiss = f64::from(noise_line[i] - previous);
            previous = noise_line[i];
            (sample(&levels, &Position::at(i, sample_rate), hiss) * 0.6) as f32
        })
        .collect()
}
