//! Streaming resampler for one channel, on rubato's synchronous FFT resampler (a fixed ratio is
//! what it is for; its delay is a whole number of frames, so trimming it is exact). The start-up
//! delay is dropped and the tail flushed with zeros, so a whole track passes through in arbitrary
//! chunks and comes out exactly [`StreamResampler::output_len`] frames long. Not real-time: `push`
//! and `finish` grow the caller's `Vec`. After construction the resampling itself allocates
//! nothing. Used to bring any input to the model's rate and the stems back to the input's rate.

use audioadapter_buffers::direct::InterleavedSlice;
use om_engine::resample::fft_resampler;
use rubato::{Fft, Resampler};

#[derive(Debug)]
pub struct StreamResampler {
    inner: Fft<f32>,
    /// Input samples per output sample.
    step: f64,
    /// Input frames per rubato call (fixed for `FixedSync::Input`).
    chunk: usize,
    /// The input chunk being filled; the first `filled` frames are valid.
    chunk_in: Vec<f32>,
    filled: usize,
    chunk_out: Vec<f32>,
    /// Leading output frames still to drop (the filter delay).
    delay_left: usize,
    pushed: u64,
    emitted: u64,
}

impl StreamResampler {
    /// Rates are whole Hz (container and model rates are); a fractional rate is rounded.
    ///
    /// # Panics
    /// If a rate is not finite and at least 1 Hz.
    pub fn new(in_rate: f64, out_rate: f64) -> Self {
        assert!(
            in_rate.is_finite() && out_rate.is_finite() && in_rate >= 1.0 && out_rate >= 1.0,
            "sample rates must be finite and at least 1 Hz"
        );
        let (rate_in, rate_out) = (in_rate.round(), out_rate.round());
        let inner = fft_resampler(rate_in as usize, rate_out as usize, 1)
            .expect("whole, positive rates construct");
        let chunk = inner.input_frames_next();
        Self {
            step: rate_in / rate_out,
            chunk,
            chunk_in: vec![0.0; chunk],
            filled: 0,
            chunk_out: vec![0.0; inner.output_frames_max()],
            delay_left: inner.output_delay(),
            inner,
            pushed: 0,
            emitted: 0,
        }
    }

    /// Output frames for `n_in` input frames: `round(n_in / step)`.
    pub fn output_len(in_rate: f64, out_rate: f64, n_in: u64) -> u64 {
        (n_in as f64 * out_rate.round() / in_rate.round()).round() as u64
    }

    /// Resamples `chunk_in` (already zero-padded to a whole chunk) and appends at most `limit`
    /// frames to `out` in total, after dropping the filter delay.
    fn run_chunk(&mut self, out: &mut Vec<f32>, limit: u64) {
        let frames_out = self.inner.output_frames_next();
        let input = InterleavedSlice::new(&self.chunk_in[..], 1, self.chunk)
            .expect("one channel, one chunk");
        let mut output =
            InterleavedSlice::new_mut(&mut self.chunk_out[..frames_out], 1, frames_out)
                .expect("one channel, one chunk");
        let (_, written) = self
            .inner
            .process_into_buffer(&input, &mut output, None)
            .expect("buffers are sized from the resampler's own limits");
        let skip = self.delay_left.min(written);
        self.delay_left -= skip;
        let room = limit.saturating_sub(self.emitted) as usize;
        let take = (written - skip).min(room);
        out.extend_from_slice(&self.chunk_out[skip..skip + take]);
        self.emitted += take as u64;
    }

    /// Feeds input; appends every output frame that is now fully determined.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.pushed += input.len() as u64;
        let mut rest = input;
        while !rest.is_empty() {
            let take = rest.len().min(self.chunk - self.filled);
            self.chunk_in[self.filled..self.filled + take].copy_from_slice(&rest[..take]);
            self.filled += take;
            rest = &rest[take..];
            if self.filled == self.chunk {
                self.filled = 0;
                self.run_chunk(out, u64::MAX);
            }
        }
    }

    /// Ends the stream (zero-extended) and appends the remaining frames so the total is exactly
    /// [`Self::output_len`] of everything pushed.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let total = (self.pushed as f64 / self.step).round() as u64;
        while self.emitted < total {
            self.chunk_in[self.filled..].fill(0.0);
            self.filled = 0;
            self.run_chunk(out, total);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

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
        // The step at each end rings for a few hundred frames; the interior must be flat.
        assert!(y[600..y.len() - 600].iter().all(|v| (v - 0.3).abs() < 1e-4));
    }
}
