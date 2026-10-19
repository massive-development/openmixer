//! Streaming windowed-sinc resampler for one channel. Kaiser-windowed sinc, centred (no group
//! delay), exact output length, no state beyond a short history, so a whole track can pass through
//! it in chunks. Not real-time: it allocates. Used to bring any input to the model's rate and the
//! stems back to the input's rate.

use std::f64::consts::PI;
use std::sync::OnceLock;

/// Zero crossings kept on each side of the kernel centre (at the lower of the two rates).
const HALF_WIDTH: usize = 24;
/// Kernel table entries per input-sample of distance.
const TABLE_RES: usize = 512;
const KAISER_BETA: f64 = 9.0;

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..60 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < 1e-14 * sum {
            break;
        }
    }
    sum
}

/// Kaiser-windowed sinc `h(d)` for `d` in `[0, HALF_WIDTH]` (units of the cutoff period), sampled
/// at `TABLE_RES` points per unit, plus one guard entry.
fn kernel_table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let norm = bessel_i0(KAISER_BETA);
        (0..=HALF_WIDTH * TABLE_RES + 1)
            .map(|i| {
                let d = i as f64 / TABLE_RES as f64;
                let w = if d >= HALF_WIDTH as f64 {
                    0.0
                } else {
                    let r = d / HALF_WIDTH as f64;
                    bessel_i0(KAISER_BETA * (1.0 - r * r).sqrt()) / norm
                };
                let s = if d == 0.0 {
                    1.0
                } else {
                    (PI * d).sin() / (PI * d)
                };
                (s * w) as f32
            })
            .collect()
    })
}

#[derive(Debug, Clone)]
pub struct StreamResampler {
    /// Input samples per output sample.
    step: f64,
    /// Kernel stretch: cutoff as a fraction of the input Nyquist (`1 / step` when downsampling).
    cutoff: f64,
    half_taps: i64,
    history: Vec<f32>,
    /// Absolute input index of `history[0]`.
    history_start: i64,
    pushed: i64,
    emitted: i64,
}

impl StreamResampler {
    pub fn new(in_rate: f64, out_rate: f64) -> Self {
        let step = in_rate / out_rate;
        let cutoff = (1.0 / step).min(1.0);
        Self {
            step,
            cutoff,
            half_taps: (HALF_WIDTH as f64 / cutoff).ceil() as i64,
            history: Vec::new(),
            history_start: 0,
            pushed: 0,
            emitted: 0,
        }
    }

    /// Output frames for `n_in` input frames: `round(n_in / step)`.
    pub fn output_len(in_rate: f64, out_rate: f64, n_in: u64) -> u64 {
        (n_in as f64 * out_rate / in_rate).round() as u64
    }

    fn sample_at(&self, pos: f64) -> f32 {
        let table = kernel_table();
        let centre = pos.floor() as i64;
        let first = centre - self.half_taps + 1;
        let mut acc = 0.0f32;
        let mut weight_sum = 0.0f32;
        for idx in first..first + 2 * self.half_taps {
            let d = (pos - idx as f64).abs() * self.cutoff * TABLE_RES as f64;
            let i = d as usize;
            if i >= HALF_WIDTH * TABLE_RES {
                continue;
            }
            let frac = (d - i as f64) as f32;
            let w = table[i] + (table[i + 1] - table[i]) * frac;
            weight_sum += w;
            let rel = idx - self.history_start;
            if rel >= 0 && (rel as usize) < self.history.len() {
                acc += w * self.history[rel as usize];
            }
        }
        if weight_sum.abs() > 1e-6 {
            acc / weight_sum
        } else {
            acc
        }
    }

    fn drain(&mut self, out: &mut Vec<f32>) {
        loop {
            let pos = self.emitted as f64 * self.step;
            // Every tap up to `floor(pos) + half_taps` must already be pushed.
            if pos.floor() as i64 + self.half_taps >= self.pushed {
                break;
            }
            out.push(self.sample_at(pos));
            self.emitted += 1;
        }
        let keep_from = (self.emitted as f64 * self.step).floor() as i64 - self.half_taps;
        let drop = (keep_from - self.history_start).clamp(0, self.history.len() as i64) as usize;
        if drop > 0 {
            self.history.drain(..drop);
            self.history_start += drop as i64;
        }
    }

    /// Feeds input; appends every output frame that is now fully determined.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.history.extend_from_slice(input);
        self.pushed += input.len() as i64;
        self.drain(out);
    }

    /// Ends the stream (zero-extended) and appends the remaining frames so the total is exactly
    /// [`Self::output_len`] of everything pushed.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let total = (self.pushed as f64 / self.step).round() as i64;
        // Zero-extension past the end is implicit: `sample_at` skips missing history.
        while self.emitted < total {
            out.push(self.sample_at(self.emitted as f64 * self.step));
            self.emitted += 1;
        }
        self.history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / rate).sin() as f32 * 0.5)
            .collect()
    }

    fn run(rin: f64, rout: f64, x: &[f32], chunk: usize) -> Vec<f32> {
        let mut r = StreamResampler::new(rin, rout);
        let mut out = Vec::new();
        for c in x.chunks(chunk) {
            r.push(c, &mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn length_is_exact_and_chunking_does_not_matter() {
        let x = sine(1000.0, 48_000.0, 48_000);
        let a = run(48_000.0, 44_100.0, &x, 1000);
        let b = run(48_000.0, 44_100.0, &x, 7777);
        assert_eq!(
            a.len() as u64,
            StreamResampler::output_len(48_000.0, 44_100.0, 48_000)
        );
        assert_eq!(a, b);
    }

    #[test]
    fn sine_survives_down_and_up() {
        for (r1, r2) in [
            (48_000.0, 44_100.0),
            (96_000.0, 44_100.0),
            (22_050.0, 44_100.0),
        ] {
            let x = sine(1000.0, r1, (r1 as usize) / 2);
            let mid = run(r1, r2, &x, 4096);
            let back = run(r2, r1, &mid, 4096);
            assert_eq!(back.len(), x.len());
            let margin = 2000.min(x.len() / 4);
            let err = x[margin..x.len() - margin]
                .iter()
                .zip(&back[margin..])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(err < 2e-3, "{r1}->{r2}: max error {err}");
        }
    }

    #[test]
    fn dc_is_preserved() {
        let x = vec![0.3f32; 5000];
        let y = run(44_100.0, 48_000.0, &x, 999);
        assert!(y[100..y.len() - 100].iter().all(|v| (v - 0.3).abs() < 1e-4));
    }
}
