//! Hann-windowed magnitude STFT over a mono clip. Port of `src/analysis/Stft.{h,cpp}`. Frame `i`
//! covers samples `[i * hop, i * hop + frame_size)`; the tail is zero padded. Magnitudes are
//! normalised so a full-scale sine peaks near 1. Holds scratch buffers, so use one instance per
//! thread.

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::f32::consts::PI;
use std::sync::Arc;

pub struct MagnitudeStft {
    frame_size: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    scale: f32,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
}

impl MagnitudeStft {
    /// `fft_order` gives `1 << fft_order` samples per frame; the hop is at least one sample.
    pub fn new(fft_order: u32, hop_size: usize) -> Self {
        let frame_size = 1usize << fft_order;
        let hop = hop_size.max(1);
        let window: Vec<f32> = (0..frame_size)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / frame_size as f32).cos())
            .collect();
        let sum = window.iter().fold(0.0f32, |acc, &w| acc + w);
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(frame_size);
        let input = fft.make_input_vec();
        let spectrum = fft.make_output_vec();
        Self {
            frame_size,
            hop,
            fft,
            window,
            scale: 2.0 / sum,
            input,
            spectrum,
        }
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    pub fn hop(&self) -> usize {
        self.hop
    }

    pub fn num_bins(&self) -> usize {
        self.frame_size / 2 + 1
    }

    /// Frames needed to cover `num_samples`; zero for an empty clip, one for a clip shorter than a frame.
    pub fn num_frames(&self, num_samples: usize) -> usize {
        if num_samples == 0 {
            0
        } else if num_samples <= self.frame_size {
            1
        } else {
            1 + (num_samples - self.frame_size).div_ceil(self.hop)
        }
    }

    pub fn bin_hz(&self, bin: usize, sample_rate: f64) -> f64 {
        bin as f64 * sample_rate / self.frame_size as f64
    }

    /// Writes `num_bins()` magnitudes of frame `index` into `magnitudes`.
    pub fn compute_frame(&mut self, samples: &[f32], index: usize, magnitudes: &mut Vec<f32>) {
        self.input.fill(0.0);
        let start = index * self.hop;
        for i in 0..self.frame_size {
            let Some(&sample) = samples.get(start + i) else {
                break;
            };
            self.input[i] = sample * self.window[i];
        }

        self.fft
            .process(&mut self.input, &mut self.spectrum)
            .expect("buffers are sized from the planner at construction");
        magnitudes.clear();
        magnitudes.extend(self.spectrum.iter().map(|bin| bin.norm() * self.scale));
    }
}
