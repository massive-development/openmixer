//! A synthetic loop for the demo library and the waveform screenshots: a four-on-the-floor kick,
//! an offbeat bass that opens up over eight bars, hats on the eighths, a mid stab on the last two
//! bars of every four and a quiet four-bar breakdown in the middle. It exists so the waveform has
//! real shape (tall low bursts, thin high lines, a quiet gap) where a sine sweep draws a flat
//! block. Generated here, deterministic, no samples from any recording.

use std::f64::consts::PI;

const KICK_GAIN: f32 = 0.8;
const KICK_HZ: f64 = 55.0;
const KICK_DECAY: f64 = 25.0;
const KICK_SECONDS: f64 = 0.15;

const BASS_HZ: f64 = 55.0;
/// The bass amplitude ramps up, and its low-pass opens, over this many bars, then starts over.
const BASS_RAMP_BARS: f64 = 8.0;
const BASS_GAIN: (f32, f32) = (0.25, 0.6);
const BASS_CUTOFF_HZ: (f64, f64) = (150.0, 2500.0);
const BASS_BEATS: f64 = 0.4;

const HAT_GAIN: f32 = 0.3;
const HAT_DECAY_SECONDS: f64 = 0.025;
const HAT_SECONDS: f64 = 0.12;
const OPEN_HAT_GAIN: f32 = 0.34;
const OPEN_HAT_DECAY_SECONDS: f64 = 0.11;
const OPEN_HAT_SECONDS: f64 = 0.4;

const STAB_HZ: [f64; 4] = [415.0, 523.0, 659.0, 988.0];
const STAB_GAIN: f32 = 0.11;
const STAB_DECAY: f64 = 10.0;
const STAB_SECONDS: f64 = 0.3;

const BARS_PER_PHRASE: usize = 4;
const BREAKDOWN_BARS: usize = 4;
/// Shortest loop (in bars) that gets a breakdown.
const MIN_BARS_FOR_BREAKDOWN: usize = 16;
const PEAK: f32 = 0.9;
const SEED: u64 = 0x0DE_AD10;

/// The breakdown of a loop of `total_bars` bars: its first bar and its length (0 when the loop is
/// too short). It starts on a phrase of four bars, around the middle.
pub fn breakdown_bars(total_bars: usize) -> (usize, usize) {
    if total_bars < MIN_BARS_FOR_BREAKDOWN {
        return (0, 0);
    }
    let first = total_bars / 2 / BARS_PER_PHRASE * BARS_PER_PHRASE;
    (first, BREAKDOWN_BARS)
}

/// Deterministic noise in `[-1, 1)` (splitmix64); not compatible with any other generator.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        ((z >> 40) as f32) * (2.0 / 16_777_216.0) - 1.0
    }
}

/// Adds `voice(t)` for `length` seconds starting at `start` (seconds).
fn add(out: &mut [f32], rate: f64, start: f64, length: f64, mut voice: impl FnMut(f64) -> f32) {
    let first = (start * rate) as usize;
    let count = (length * rate) as usize;
    for i in 0..count {
        let Some(sample) = out.get_mut(first + i) else {
            break;
        };
        *sample += voice(i as f64 / rate);
    }
}

fn add_kick(out: &mut [f32], rate: f64, start: f64) {
    add(out, rate, start, KICK_SECONDS, |t| {
        KICK_GAIN * ((-t * KICK_DECAY).exp() * (2.0 * PI * KICK_HZ * t).sin()) as f32
    });
}

/// A saw at [`BASS_HZ`] through a one-pole low-pass; `ramp` (0..1) opens the filter and the level.
fn add_bass(out: &mut [f32], rate: f64, start: f64, beat: f64, ramp: f64) {
    let cutoff = BASS_CUTOFF_HZ.0 + (BASS_CUTOFF_HZ.1 - BASS_CUTOFF_HZ.0) * ramp * ramp;
    let coefficient = 1.0 - (-2.0 * PI * cutoff / rate).exp();
    let gain = BASS_GAIN.0 + (BASS_GAIN.1 - BASS_GAIN.0) * ramp as f32;
    let length = BASS_BEATS * beat;
    let mut filtered = 0.0f64;
    add(out, rate, start, length, |t| {
        let saw = 2.0 * (t * BASS_HZ).fract() - 1.0;
        filtered += coefficient * (saw - filtered);
        // 4 ms attack and a 20 ms release keep the edges click free.
        let envelope = (t / 0.004).min(1.0) * ((length - t) / 0.02).clamp(0.0, 1.0);
        gain * (filtered * envelope) as f32
    });
}

/// Differenced noise (a high-pass) with an exponential decay.
fn add_hat(out: &mut [f32], rate: f64, start: f64, noise: &mut Noise, open: bool) {
    let (gain, decay, length) = if open {
        (OPEN_HAT_GAIN, OPEN_HAT_DECAY_SECONDS, OPEN_HAT_SECONDS)
    } else {
        (HAT_GAIN, HAT_DECAY_SECONDS, HAT_SECONDS)
    };
    let mut last = 0.0f32;
    add(out, rate, start, length, |t| {
        let x = noise.next();
        let high = 0.5 * (x - last);
        last = x;
        gain * high * (-t / decay).exp() as f32
    });
}

fn add_stab(out: &mut [f32], rate: f64, start: f64) {
    add(out, rate, start, STAB_SECONDS, |t| {
        let body: f64 = STAB_HZ.iter().map(|hz| (2.0 * PI * hz * t).sin()).sum();
        STAB_GAIN * ((-t * STAB_DECAY).exp() * body) as f32
    });
}

/// `seconds` of the loop at `bpm`, mono at `rate`, peak 0.9. Empty for no length; silent for a
/// tempo that is not positive.
pub fn demo_loop(seconds: f64, bpm: f64, rate: f64) -> Vec<f32> {
    let total = (seconds.max(0.0) * rate) as usize;
    let mut out = vec![0.0f32; total];
    if bpm <= 0.0 || total == 0 {
        return out;
    }
    let beat = 60.0 / bpm;
    let bar = beat * 4.0;
    let bars = (seconds / bar).ceil() as usize;
    let (cut_from, cut_len) = breakdown_bars(((seconds / bar) as usize).max(1));
    let mut noise = Noise(SEED);

    for index in 0..bars {
        let start = index as f64 * bar;
        let broken = (cut_from..cut_from + cut_len).contains(&index);
        let ramp = (index as f64 % BASS_RAMP_BARS) / BASS_RAMP_BARS;
        if !broken {
            for b in 0..4 {
                let t = start + f64::from(b) * beat;
                add_kick(&mut out, rate, t);
                add_bass(&mut out, rate, t + 0.5 * beat, beat, ramp);
            }
        }
        for step in 0..8 {
            let t = start + f64::from(step) * beat / 2.0;
            add_hat(&mut out, rate, t, &mut noise, step % 4 == 3);
        }
        if index % BARS_PER_PHRASE >= 2 {
            add_stab(&mut out, rate, start);
            add_stab(&mut out, rate, start + 1.5 * beat);
        }
    }

    let top = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if top > 0.0 {
        let scale = PEAK / top;
        for x in &mut out {
            *x *= scale;
        }
    }
    out
}

#[cfg(test)]
mod tests;
