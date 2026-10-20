//! The ONNX Runtime network behind [`StemInference`] (feature `onnx`). Safe `ort` API only, so the
//! crate keeps `forbid(unsafe_code)`. The model file comes from the verified download
//! ([`crate::WeightManager`]); its tensor names and shapes are the [`ModelContract`].

use crate::contract::ModelContract;
use crate::pipeline::{SegmentInput, SegmentOutput, StemInference};
use crate::separator::StemError;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{TensorElementType, TensorRef, ValueType};
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
        check_io(&session, &contract)?;
        Ok(Self { session, contract })
    }
}

/// Checks the session's declared inputs and outputs against the contract, so a wrong name or shape
/// in the manifest is a precise load error rather than a failed or panicking run. Dimensions the
/// model leaves dynamic (`-1`) match anything; the mix input must be the only other input.
fn check_io(session: &Session, c: &ModelContract) -> Result<(), StemError> {
    let bins = c.n_fft / 2;
    let mut want_inputs: Vec<(&str, Vec<Option<usize>>)> = vec![(
        c.mix_input.as_str(),
        vec![Some(1), Some(2), Some(c.segment_samples)],
    )];
    if let Some(name) = &c.spec_input {
        want_inputs.push((name.as_str(), vec![Some(1), Some(4), Some(bins), None]));
    }
    let names = |outlets: &[ort::value::Outlet]| {
        outlets
            .iter()
            .map(|o| o.name().to_owned())
            .collect::<Vec<_>>()
            .join(", ")
    };
    for outlet in session.inputs() {
        if !want_inputs.iter().any(|(n, _)| *n == outlet.name()) {
            let wanted: Vec<&str> = want_inputs.iter().map(|(n, _)| *n).collect();
            return Err(StemError::Invalid(format!(
                "model input {:?} is not in the contract, which wants {wanted:?}",
                outlet.name(),
            )));
        }
    }
    for (name, want) in &want_inputs {
        let outlet = session
            .inputs()
            .iter()
            .find(|o| o.name() == *name)
            .ok_or_else(|| {
                StemError::Invalid(format!(
                    "model has no input {name:?} (inputs: {})",
                    names(session.inputs())
                ))
            })?;
        check_tensor("input", outlet, want)?;
    }

    let mut want_outputs: Vec<(&str, Vec<Option<usize>>)> = vec![(
        c.stems_output.as_str(),
        vec![Some(1), Some(4), Some(2), Some(c.segment_samples)],
    )];
    if let Some(name) = &c.spec_output {
        want_outputs.push((
            name.as_str(),
            vec![Some(1), Some(4), Some(4), Some(bins), None],
        ));
    }
    for (name, want) in &want_outputs {
        let outlet = session
            .outputs()
            .iter()
            .find(|o| o.name() == *name)
            .ok_or_else(|| {
                StemError::Invalid(format!(
                    "model has no output {name:?} (outputs: {})",
                    names(session.outputs())
                ))
            })?;
        check_tensor("output", outlet, want)?;
    }
    Ok(())
}

fn check_tensor(
    kind: &str,
    outlet: &ort::value::Outlet,
    want: &[Option<usize>],
) -> Result<(), StemError> {
    let name = outlet.name();
    let ValueType::Tensor { ty, shape, .. } = outlet.dtype() else {
        return Err(StemError::Invalid(format!(
            "model {kind} {name:?} is not a tensor"
        )));
    };
    if *ty != TensorElementType::Float32 {
        return Err(StemError::Invalid(format!(
            "model {kind} {name:?} is {ty:?}, the contract needs float32"
        )));
    }
    let dims_ok = shape.len() == want.len()
        && shape.iter().zip(want).all(|(&actual, w)| match w {
            Some(w) => actual < 0 || usize::try_from(actual).is_ok_and(|a| a == *w),
            None => true,
        });
    if !dims_ok {
        let shape_text: Vec<i64> = shape.iter().copied().collect();
        return Err(StemError::Invalid(format!(
            "model {kind} {name:?} has shape {shape_text:?}, the contract needs {want:?} (None is any)"
        )));
    }
    Ok(())
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

        let stems_name = self.contract.stems_output.as_str();
        let (_, stems) = outputs
            .get(stems_name)
            .ok_or_else(|| StemError::Invalid(format!("model produced no output {stems_name:?}")))?
            .try_extract_tensor::<f32>()
            .map_err(ort_err)?;
        output.stems.clear();
        output.stems.extend_from_slice(stems);
        output.spec = match &self.contract.spec_output {
            Some(name) => {
                let (_, spec) = outputs
                    .get(name.as_str())
                    .ok_or_else(|| {
                        StemError::Invalid(format!("model produced no output {name:?}"))
                    })?
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
