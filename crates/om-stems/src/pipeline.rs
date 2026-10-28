//! The htdemucs separation pipeline, everything except the neural network: input resampling to the
//! model's rate, fixed-size segmenting with 25 % overlap and triangular-weight overlap-add,
//! per-segment normalisation (undone after), optional STFT/iSTFT, and resampling the stems back to
//! the input's rate. The network sits behind [`StemInference`], so all of this is tested without ONNX
//! Runtime.
//!
//! Streamed: memory is a few segments (input window, eight overlap-add accumulators, the model's
//! output), independent of the track length. The sink gets chunks of at most [`EMIT_CHUNK`] frames.
//!
//! Normalisation is per segment (mean and standard deviation of the segment's mono mix), not per
//! track as in the Demucs CLI, because a whole-track pass would need the whole track. Each stem gets
//! `mean / 4` back, so the stems still add up to the input. A silent segment is divided by a floor
//! of [`MIN_STD`], which keeps silence exactly silent.

use crate::contract::ModelContract;
use crate::resample::StreamResampler;
use crate::separator::{StemError, StemKind, StemSeparator, StemSink, StereoSource};
use crate::spectral::Spectral;
use om_analysis::AnalysisContext;
use std::sync::Mutex;

/// Largest chunk handed to a sink, in frames.
pub const EMIT_CHUNK: usize = 16_384;
/// Frames read from the source per call.
const READ_CHUNK: usize = 16_384;
/// Standard-deviation floor used when normalising (silence stays silence).
pub const MIN_STD: f32 = 1e-4;
/// Output frames held back so the final length can be trimmed to the input length exactly.
const HOLD: usize = 8;

/// One segment for the network.
pub struct SegmentInput<'a> {
    /// Segment length in frames (`contract.segment_samples`).
    pub frames: usize,
    /// Planar stereo `[left(frames) | right(frames)]`, normalised, zero-padded past the clip's end.
    pub mix: &'a [f32],
    /// Spectrum of `mix` (`[4][n_fft/2][T]`, see [`Spectral`]) when the contract wants one.
    pub spec: Option<&'a [f32]>,
}

/// What the network returns for one segment. The pipeline owns the buffers and reuses them.
#[derive(Default)]
pub struct SegmentOutput {
    /// `[stem(4)][channel(2)][frames]` in the contract's `stem_order`.
    pub stems: Vec<f32>,
    /// `[stem(4)][4][n_fft/2][T]` frequency-branch spectra, when the contract has `spec_output`.
    pub spec: Option<Vec<f32>>,
}

/// The neural network. Implemented by `OrtInference` (feature `onnx`) and by fakes in tests.
pub trait StemInference: Send {
    fn run(
        &mut self,
        input: &SegmentInput<'_>,
        output: &mut SegmentOutput,
    ) -> Result<(), StemError>;
}

pub struct HtDemucsPipeline<I: StemInference> {
    inference: I,
    contract: ModelContract,
    spectral: Option<Spectral>,
}

/// Writes model-rate blocks to the sink at the input's rate.
struct Emitter<'a> {
    sink: &'a mut dyn StemSink,
    /// One per stem channel (`kind_index * 2 + channel`); `None` when the rates already match.
    resamplers: Option<Vec<StreamResampler>>,
    pending: Vec<Vec<f32>>,
    written: u64,
}

impl<'a> Emitter<'a> {
    fn new(sink: &'a mut dyn StemSink, model_rate: f64, src_rate: f64) -> Self {
        let resamplers = ((model_rate - src_rate).abs() > 1e-6)
            .then(|| vec![StreamResampler::new(model_rate, src_rate); 8]);
        Self {
            sink,
            resamplers,
            pending: vec![Vec::new(); 8],
            written: 0,
        }
    }

    fn write_split(
        sink: &mut dyn StemSink,
        kind: StemKind,
        left: &[f32],
        right: &[f32],
    ) -> Result<(), StemError> {
        for (l, r) in left.chunks(EMIT_CHUNK).zip(right.chunks(EMIT_CHUNK)) {
            sink.write(kind, l, r)?;
        }
        Ok(())
    }

    fn emit(&mut self, block: &[Vec<f32>]) -> Result<(), StemError> {
        match &mut self.resamplers {
            None => {
                for (k, kind) in StemKind::ALL.into_iter().enumerate() {
                    Self::write_split(self.sink, kind, &block[2 * k], &block[2 * k + 1])?;
                }
                self.written += block[0].len() as u64;
                Ok(())
            }
            Some(resamplers) => {
                for (i, r) in resamplers.iter_mut().enumerate() {
                    r.push(&block[i], &mut self.pending[i]);
                }
                self.flush(HOLD)
            }
        }
    }

    fn flush(&mut self, keep: usize) -> Result<(), StemError> {
        let ready = self.pending[0].len().saturating_sub(keep);
        if ready == 0 {
            return Ok(());
        }
        for (k, kind) in StemKind::ALL.into_iter().enumerate() {
            Self::write_split(
                self.sink,
                kind,
                &self.pending[2 * k][..ready],
                &self.pending[2 * k + 1][..ready],
            )?;
        }
        self.pending.iter_mut().for_each(|p| {
            p.drain(..ready);
        });
        self.written += ready as u64;
        Ok(())
    }

    /// Flushes the resampler tails and trims or pads to exactly `total` frames.
    fn finish(&mut self, total: u64) -> Result<(), StemError> {
        if let Some(resamplers) = &mut self.resamplers {
            for (i, r) in resamplers.iter_mut().enumerate() {
                r.finish(&mut self.pending[i]);
            }
            let needed = total.saturating_sub(self.written) as usize;
            for p in &mut self.pending {
                let fill = p.last().copied().unwrap_or(0.0);
                p.resize(needed, fill);
            }
            self.flush(0)?;
        }
        debug_assert_eq!(self.written, total);
        Ok(())
    }
}

fn triangular(n: usize) -> Vec<f32> {
    (0..n).map(|i| (i + 1).min(n - i) as f32).collect()
}

impl<I: StemInference> HtDemucsPipeline<I> {
    pub fn new(inference: I, contract: ModelContract) -> Result<Self, StemError> {
        contract.validate().map_err(StemError::Invalid)?;
        let spectral = contract
            .spec_input
            .is_some()
            .then(|| Spectral::new(contract.n_fft, contract.hop));
        Ok(Self {
            inference,
            contract,
            spectral,
        })
    }

    pub fn contract(&self) -> &ModelContract {
        &self.contract
    }

    pub fn run(
        &mut self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        let src_rate = input.sample_rate();
        if !(src_rate.is_finite() && src_rate >= 1000.0) {
            return Err(StemError::Invalid(format!("sample rate {src_rate}")));
        }
        let model_rate = f64::from(self.contract.sample_rate);
        let n = self.contract.segment_samples;
        let overlap_n = ((n as f32 * self.contract.overlap).round() as usize).min(n - 1);
        let hop = n - overlap_n;
        let weights = triangular(n);
        let resample_in = (model_rate - src_rate).abs() > 1e-6;
        let mut in_resamplers = [
            StreamResampler::new(src_rate, model_rate),
            StreamResampler::new(src_rate, model_rate),
        ];
        let total_model = input
            .total_frames()
            .map(|t| StreamResampler::output_len(src_rate, model_rate, t));

        let mut left_read = vec![0.0f32; READ_CHUNK];
        let mut right_read = vec![0.0f32; READ_CHUNK];
        let mut inbuf: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
        let mut src_frames = 0u64;
        let mut eof = false;

        let mut acc = vec![vec![0.0f32; n]; 8];
        let mut wsum = vec![0.0f32; n];
        let mut mix = vec![0.0f32; 2 * n];
        let mut out = SegmentOutput::default();
        let mut block: Vec<Vec<f32>> = vec![Vec::new(); 8];
        let mut emitter = Emitter::new(sink, model_rate, src_rate);
        let mut segment_start = 0u64;
        let mut last_progress = 0.0f32;
        let mut report = |p: f32| {
            let p = p.clamp(last_progress, 1.0);
            last_progress = p;
            ctx.set_progress(p);
        };
        let mut segment_index = 0u32;

        loop {
            if ctx.is_cancelled() {
                return Err(StemError::Cancelled);
            }
            // Fill the window plus one look-ahead frame (tells whether this segment is the last).
            while inbuf[0].len() < n + 1 && !eof {
                let got = input.read(&mut left_read, &mut right_read)?;
                if got == 0 {
                    eof = true;
                    if resample_in {
                        for (c, r) in in_resamplers.iter_mut().enumerate() {
                            r.finish(&mut inbuf[c]);
                        }
                    }
                } else {
                    src_frames += got as u64;
                    if resample_in {
                        in_resamplers[0].push(&left_read[..got], &mut inbuf[0]);
                        in_resamplers[1].push(&right_read[..got], &mut inbuf[1]);
                    } else {
                        inbuf[0].extend_from_slice(&left_read[..got]);
                        inbuf[1].extend_from_slice(&right_read[..got]);
                    }
                }
            }
            let avail = inbuf[0].len();
            if avail == 0 {
                break;
            }
            let last = avail <= n;
            let valid = avail.min(n);

            // Normalise by the mono mix's mean and standard deviation.
            let (mean, std) = {
                let sum: f64 = (0..valid)
                    .map(|i| f64::from(inbuf[0][i] + inbuf[1][i]) * 0.5)
                    .sum();
                let mean = sum / valid as f64;
                let var: f64 = (0..valid)
                    .map(|i| {
                        let d = f64::from(inbuf[0][i] + inbuf[1][i]) * 0.5 - mean;
                        d * d
                    })
                    .sum::<f64>()
                    / valid as f64;
                (mean as f32, (var.sqrt() as f32).max(MIN_STD))
            };
            for c in 0..2 {
                let plane = &mut mix[c * n..(c + 1) * n];
                for i in 0..valid {
                    plane[i] = (inbuf[c][i] - mean) / std;
                }
                plane[valid..].fill(0.0);
            }
            let spec = self
                .spectral
                .as_ref()
                .map(|s| s.forward(&mix[..n], &mix[n..]));
            self.inference.run(
                &SegmentInput {
                    frames: n,
                    mix: &mix,
                    spec: spec.as_deref(),
                },
                &mut out,
            )?;
            if out.stems.len() != 8 * n {
                return Err(StemError::Inference(format!(
                    "model returned {} samples, expected {}",
                    out.stems.len(),
                    8 * n
                )));
            }
            if ctx.is_cancelled() {
                return Err(StemError::Cancelled);
            }
            // A NaN or infinite value would quantise to silence or a full-scale click; refuse the
            // whole job instead.
            let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
            if !finite(&out.stems) || out.spec.as_deref().is_some_and(|z| !finite(z)) {
                return Err(StemError::Invalid("non-finite model output".into()));
            }

            // Denormalise and overlap-add.
            let spec_stem_len = self.spectral.as_ref().map_or(0, |s| s.spec_len(n));
            let mut tail_spec: Vec<(Vec<f32>, Vec<f32>)> = Vec::new();
            if let (Some(s), Some(z)) = (&self.spectral, &out.spec) {
                if z.len() != 4 * spec_stem_len {
                    return Err(StemError::Inference(
                        "spectrum output has the wrong size".into(),
                    ));
                }
                for stem in 0..4 {
                    tail_spec
                        .push(s.inverse(&z[stem * spec_stem_len..(stem + 1) * spec_stem_len], n));
                }
            }
            for (model_idx, kind) in self.contract.stem_order.into_iter().enumerate() {
                let k = StemKind::ALL.iter().position(|&x| x == kind).unwrap_or(0);
                for c in 0..2 {
                    let src = &out.stems[(model_idx * 2 + c) * n..(model_idx * 2 + c + 1) * n];
                    let dst = &mut acc[k * 2 + c];
                    for i in 0..valid {
                        let mut v = src[i];
                        if let Some((l, r)) = tail_spec.get(model_idx) {
                            v += if c == 0 { l[i] } else { r[i] };
                        }
                        dst[i] += weights[i] * (v * std + mean * 0.25);
                    }
                }
            }
            for i in 0..valid {
                wsum[i] += weights[i];
            }

            let emit_len = if last { valid } else { hop };
            for (idx, b) in block.iter_mut().enumerate() {
                b.clear();
                b.extend((0..emit_len).map(|i| acc[idx][i] / wsum[i]));
            }
            emitter.emit(&block)?;
            if last {
                break;
            }
            for a in acc.iter_mut() {
                a.copy_within(hop..n, 0);
                a[n - hop..].fill(0.0);
            }
            wsum.copy_within(hop..n, 0);
            wsum[n - hop..].fill(0.0);
            for b in inbuf.iter_mut() {
                b.drain(..hop);
            }
            segment_start += hop as u64;
            segment_index += 1;
            report(match total_model {
                Some(t) if t > 0 => (segment_start as f32 / t as f32).min(0.99),
                _ => 1.0 - 1.0 / (1.0 + segment_index as f32),
            });
        }
        emitter.finish(src_frames)?;
        report(1.0);
        Ok(())
    }
}

/// A [`StemSeparator`] around a pipeline. The network needs `&mut`, so calls serialise on a mutex
/// (one separation at a time per model instance, which is what one ONNX session wants anyway).
pub struct HtDemucsSeparator<I: StemInference> {
    id: String,
    version: i32,
    native_rate: f64,
    pipeline: Mutex<HtDemucsPipeline<I>>,
}

impl<I: StemInference> HtDemucsSeparator<I> {
    pub fn new(
        id: impl Into<String>,
        version: i32,
        inference: I,
        contract: ModelContract,
    ) -> Result<Self, StemError> {
        let native_rate = f64::from(contract.sample_rate);
        Ok(Self {
            id: id.into(),
            version,
            native_rate,
            pipeline: Mutex::new(HtDemucsPipeline::new(inference, contract)?),
        })
    }
}

impl<I: StemInference> StemSeparator for HtDemucsSeparator<I> {
    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> i32 {
        self.version
    }

    fn native_rate(&self) -> f64 {
        self.native_rate
    }

    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        let mut pipeline = match self.pipeline.lock() {
            Ok(guard) => guard,
            Err(_) => {
                // An earlier job panicked inside the network. Clear the poison so later jobs work;
                // this one fails, and the next run starts from a fresh `run` on the same network.
                self.pipeline.clear_poison();
                return Err(StemError::Inference(
                    "the separator recovered from a panic in an earlier job; retry".into(),
                ));
            }
        };
        pipeline.run(input, sink, ctx)
    }
}
