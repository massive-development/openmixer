//! THROWAWAY SPIKE: deterministic synthetic test material (no real music, no licence issues).
use crate::SR;
use std::f64::consts::PI;

/// Fundamentals of the sustained chord (A3 + C#4 + E4, just-ish equal tempered).
pub const CHORD_HZ: [f64; 3] = [220.0, 277.18, 329.63];

/// Sustained chord: three fundamentals plus a quiet 2nd harmonic, slow tremolo-free (steady),
/// 50 ms fades at the ends. Peak ~0.4.
pub fn chord(secs: f64) -> Vec<f32> {
    let n = (secs * SR as f64) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let mut s = 0.0;
            for f in CHORD_HZ {
                s += (2.0 * PI * f * t).sin() + 0.3 * (2.0 * PI * 2.0 * f * t).sin();
            }
            let fade = (t / 0.05).min(1.0).min(((secs - t) / 0.05).max(0.0));
            (s * 0.1 * fade) as f32
        })
        .collect()
}

/// Tiny deterministic noise source.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0
    }
}

/// Drum-like pattern at 120 BPM (beat = 0.5 s): kick (decaying 150->50 Hz sweep) on every beat,
/// sharp click+noise "snare" on beats 2 and 4, short hi-hat noise bursts on 8ths, one isolated
/// single-sample-wide click on the "and" of 4. Peak ~0.5.
pub fn drums(secs: f64) -> Vec<f32> {
    let n = (secs * SR as f64) as usize;
    let mut out = vec![0.0f32; n];
    let mut rng = Lcg(12345);
    let beat = 0.5;
    let mut add = |start: f64, len: f64, f: &mut dyn FnMut(f64) -> f64| {
        let s0 = (start * SR as f64) as usize;
        for k in 0..((len * SR as f64) as usize) {
            if s0 + k < n {
                out[s0 + k] += f(k as f64 / SR as f64) as f32;
            }
        }
    };
    let nbeats = (secs / beat) as usize;
    for b in 0..nbeats {
        let t0 = b as f64 * beat;
        // kick
        add(t0, 0.25, &mut |t| {
            let ph = 2.0 * PI * (50.0 * t + 100.0 * (1.0 - (-t * 30.0).exp()) / 30.0);
            0.5 * ph.sin() * (-t * 14.0).exp()
        });
        // snare on 2 and 4
        if b % 2 == 1 {
            add(t0, 0.15, &mut |t| 0.3 * rng.next() * (-t * 35.0).exp());
        }
        // 8th hat
        add(t0 + beat / 2.0, 0.03, &mut |t| 0.15 * rng.next() * (-t * 120.0).exp());
        // sharp click on the last 8th of each bar
        if b % 4 == 3 {
            add(t0 + beat * 0.75, 0.0005, &mut |_| 0.5);
        }
    }
    out
}

/// Pure sine for tests.
pub fn sine(hz: f64, secs: f64) -> Vec<f32> {
    (0..(secs * SR as f64) as usize)
        .map(|i| (0.5 * (2.0 * PI * hz * i as f64 / SR as f64).sin()) as f32)
        .collect()
}
