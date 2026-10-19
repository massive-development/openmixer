//! The ONNX Runtime network behind [`StemInference`] (feature `onnx`). Safe `ort` API only, so the
//! crate keeps `forbid(unsafe_code)`. The model file comes from the verified download
//! ([`crate::WeightManager`]); its tensor names and shapes are the [`ModelContract`].

use crate::contract::ModelContract;
use crate::pipeline::{SegmentInput, SegmentOutput, StemInference};
use crate::separator::StemError;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

pub struct OrtInference {
    session: Session,
    contract: ModelContract,
}

fn ort_err(e: impl std::fmt::Display) -> StemError {
    StemError::Inference(e.to_string())
}

/// Half the cores (at least one): separation runs beside the audio engine and the UI.
pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get() / 2)
        .max(1)
}

impl OrtInference {
    /// Loads `model` (an ONNX file already verified against its manifest). `threads` defaults to
    /// [`default_threads`] and is capped at the number of cores.
    pub fn load(
        model: &Path,
        contract: ModelContract,
        threads: Option<usize>,
    ) -> Result<Self, StemError> {
        contract.validate().map_err(StemError::Invalid)?;
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let threads = threads.unwrap_or_else(default_threads).clamp(1, cores);
        let session = Session::builder()
            .map_err(ort_err)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_err)?
            .with_intra_threads(threads)
            .map_err(ort_err)?
            .commit_from_file(model)
            .map_err(ort_err)?;
        Ok(Self { session, contract })
    }
}

impl StemInference for OrtInference {
    fn run(
        &mut self,
        input: &SegmentInput<'_>,
        output: &mut SegmentOutput,
    ) -> Result<(), StemError> {
        let n = input.frames;
        let mix = TensorRef::from_array_view(([1i64, 2, n as i64], input.mix)).map_err(ort_err)?;
        let outputs = match (&self.contract.spec_input, input.spec) {
            (Some(name), Some(spec)) => {
                let bins = (self.contract.n_fft / 2) as i64;
                let frames = spec.len() as i64 / (4 * bins);
                let z =
                    TensorRef::from_array_view(([1i64, 4, bins, frames], spec)).map_err(ort_err)?;
                self.session
                    .run(ort::inputs![self.contract.mix_input.as_str() => mix, name.as_str() => z])
            }
            _ => self
                .session
                .run(ort::inputs![self.contract.mix_input.as_str() => mix]),
        }
        .map_err(ort_err)?;

        let (_, stems) = outputs[self.contract.stems_output.as_str()]
            .try_extract_tensor::<f32>()
            .map_err(ort_err)?;
        output.stems.clear();
        output.stems.extend_from_slice(stems);
        output.spec = match &self.contract.spec_output {
            Some(name) => {
                let (_, spec) = outputs[name.as_str()]
                    .try_extract_tensor::<f32>()
                    .map_err(ort_err)?;
                let mut v = output.spec.take().unwrap_or_default();
                v.clear();
                v.extend_from_slice(spec);
                Some(v)
            }
            None => None,
        };
        Ok(())
    }
}
