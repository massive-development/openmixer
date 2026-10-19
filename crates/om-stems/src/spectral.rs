//! STFT and inverse STFT in the layout of Demucs' hybrid models ("complex as channels").
//!
//! Conventions, taken from the Demucs `HTDemucs._spec` / `_ispec` source as remembered offline and
//! NOT yet checked against an exported model (see [`crate::ModelContract`]):
//! periodic Hann window, `n_fft` 4096, hop 1024, `normalized=True` (forward scaled by
//! `1/sqrt(n_fft)`), `center=True` with reflect padding, applied after a first reflect pad of
//! `hop/2*3` samples (plus the tail needed to reach `ceil(len/hop)*hop`); the Nyquist bin is
//! dropped and the first two and last frames are cut, leaving `n_fft/2` bins and `ceil(len/hop)`
//! frames. The inverse re-adds a zero Nyquist bin and two zero frames per side, runs a normalised
//! overlap-add inverse, and crops `hop/2*3` from the start.
//!
//! Layout of a spectrum buffer: `[4][bins][frames]` flattened, channel order
//! `left.re, left.im, right.re, right.im`.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f32::consts::PI;
use std::sync::Arc;

pub struct Spectral {
    n_fft: usize,
    hop: usize,
    window: Vec<f32>,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
}

/// `numpy`/`torch` "reflect" index (edge sample not repeated), valid for any offset.
fn reflect_index(i: i64, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let period = 2 * (len as i64 - 1);
    let m = i.rem_euclid(period);
    (if m < len as i64 { m } else { period - m }) as usize
}

fn reflect_pad(x: &[f32], before: usize, after: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len() + before + after);
    for i in 0..(x.len() + before + after) {
        out.push(x[reflect_index(i as i64 - before as i64, x.len())]);
    }
    out
}

impl Spectral {
    pub fn new(n_fft: usize, hop: usize) -> Self {
        assert!(n_fft.is_power_of_two() && hop > 0 && n_fft % hop == 0 && hop % 2 == 0);
        let window = (0..n_fft)
            .map(|n| 0.5 - 0.5 * (2.0 * PI * n as f32 / n_fft as f32).cos())
            .collect();
        let mut planner = RealFftPlanner::<f32>::new();
        Self {
            n_fft,
            hop,
            window,
            fwd: planner.plan_fft_forward(n_fft),
            inv: planner.plan_fft_inverse(n_fft),
        }
    }

    pub fn bins(&self) -> usize {
        self.n_fft / 2
    }

    pub fn frames(&self, len: usize) -> usize {
        len.div_ceil(self.hop)
    }

    /// Elements of a spectrum buffer for `len` samples.
    pub fn spec_len(&self, len: usize) -> usize {
        4 * self.bins() * self.frames(len)
    }

    fn forward_channel(&self, x: &[f32], out: &mut [f32], channel: usize) {
        let len = x.len();
        let frames = self.frames(len);
        let bins = self.bins();
        let pad = self.hop / 2 * 3;
        let stage1 = reflect_pad(x, pad, pad + frames * self.hop - len);
        let stage2 = reflect_pad(&stage1, self.n_fft / 2, self.n_fft / 2);
        let scale = 1.0 / (self.n_fft as f32).sqrt();
        let mut frame = vec![0.0f32; self.n_fft];
        let mut spectrum = self.fwd.make_output_vec();
        let (re_base, im_base) = (
            2 * channel * bins * frames,
            (2 * channel + 1) * bins * frames,
        );
        for t in 0..frames {
            // Frame index 2 + t of the full STFT.
            let start = (t + 2) * self.hop;
            for (i, v) in frame.iter_mut().enumerate() {
                *v = stage2[start + i] * self.window[i];
            }
            self.fwd
                .process(&mut frame, &mut spectrum)
                .expect("buffer sizes are fixed by the plan");
            for b in 0..bins {
                out[re_base + b * frames + t] = spectrum[b].re * scale;
                out[im_base + b * frames + t] = spectrum[b].im * scale;
            }
        }
    }

    /// Spectrum of a stereo signal (`left.len() == right.len()`).
    pub fn forward(&self, left: &[f32], right: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; self.spec_len(left.len())];
        self.forward_channel(left, &mut out, 0);
        self.forward_channel(right, &mut out, 1);
        out
    }

    fn inverse_channel(&self, z: &[f32], channel: usize, len: usize, out: &mut [f32]) {
        let frames = self.frames(len);
        let bins = self.bins();
        let pad = self.hop / 2 * 3;
        let total_frames = frames + 4;
        let full_len = self.n_fft + self.hop * (total_frames - 1);
        let mut acc = vec![0.0f32; full_len];
        let mut env = vec![0.0f32; full_len];
        let (re_base, im_base) = (
            2 * channel * bins * frames,
            (2 * channel + 1) * bins * frames,
        );
        let mut spectrum = self.inv.make_input_vec();
        let mut frame = vec![0.0f32; self.n_fft];
        let scale = 1.0 / (self.n_fft as f32).sqrt();
        for t in 0..total_frames {
            spectrum
                .iter_mut()
                .for_each(|c| *c = Complex::new(0.0, 0.0));
            if (2..2 + frames).contains(&t) {
                let k = t - 2;
                for b in 0..bins {
                    spectrum[b] =
                        Complex::new(z[re_base + b * frames + k], z[im_base + b * frames + k]);
                }
            }
            spectrum[0].im = 0.0;
            // The Nyquist bin (index bins) stays zero, as in Demucs.
            self.inv
                .process(&mut spectrum, &mut frame)
                .expect("buffer sizes are fixed by the plan");
            let start = t * self.hop;
            for i in 0..self.n_fft {
                acc[start + i] += frame[i] * scale * self.window[i];
                env[start + i] += self.window[i] * self.window[i];
            }
        }
        let offset = self.n_fft / 2 + pad;
        for (i, o) in out.iter_mut().enumerate() {
            let e = env[offset + i];
            *o = if e > 1e-11 { acc[offset + i] / e } else { 0.0 };
        }
    }

    /// Inverse of [`Self::forward`] for `len` samples per channel.
    pub fn inverse(&self, z: &[f32], len: usize) -> (Vec<f32>, Vec<f32>) {
        assert_eq!(z.len(), self.spec_len(len), "spectrum size");
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        self.inverse_channel(z, 0, len, &mut left);
        self.inverse_channel(z, 1, len, &mut right);
        (left, right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, f: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * f * i as f32 / 44100.0).sin() * 0.5)
            .collect()
    }

    #[test]
    fn shapes_match_htdemucs() {
        let s = Spectral::new(4096, 1024);
        assert_eq!(s.frames(343_980), 336);
        assert_eq!(s.bins(), 2048);
    }

    #[test]
    fn round_trip_recovers_the_signal() {
        let s = Spectral::new(512, 128);
        for n in [3000usize, 5000] {
            let l = tone(n, 440.0);
            let r = tone(n, 1234.0);
            let z = s.forward(&l, &r);
            let (l2, r2) = s.inverse(&z, n);
            // As in Demucs, the first two and last frames are cut, so the edges (about 1.5
            // windows) are attenuated rather than exact; the interior must be.
            let m = 800;
            let err = l[m..n - m]
                .iter()
                .zip(&l2[m..n - m])
                .chain(r[m..n - m].iter().zip(&r2[m..n - m]))
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(err < 1e-3, "n={n} err={err}");
        }
    }

    #[test]
    fn spectrum_is_linear_and_peaks_at_the_tone() {
        let s = Spectral::new(512, 128);
        let l = tone(2048, 86.1328125 * 10.0); // bin 10 of a 512 FFT at 44.1 kHz
        let z = s.forward(&l, &vec![0.0; 2048]);
        let frames = s.frames(2048);
        let mag = |b: usize| {
            let t = frames / 2;
            let re = z[b * frames + t];
            let im = z[(s.bins() * frames) + b * frames + t];
            re.hypot(im)
        };
        assert!(mag(10) > 5.0 * mag(40));
    }
}
