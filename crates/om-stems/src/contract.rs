//! The I/O contract between the pipeline and an exported separation model.
//!
//! Nothing here was checked against a real exported htdemucs graph: that export is a user step
//! (`tools/export_htdemucs.py`) and the file is not in the repository. The contract is therefore
//! explicit data that travels in the model manifest, and the default ([`ModelContract::htdemucs`])
//! describes the variant `tools/export_htdemucs.py` produces:
//!
//! * input `mix`, `float32 [1, 2, segment_samples]`, the stereo segment at `sample_rate`,
//!   already normalised by the pipeline (mean removed, divided by the standard deviation);
//! * output `stems`, `float32 [1, 4, 2, segment_samples]`, in `stem_order`;
//! * the STFT and inverse STFT run INSIDE the exported graph (Demucs' time and frequency branches
//!   are summed there), so no spectrogram crosses the boundary.
//!
//! The plan's Open Decision 2 prefers the STFT outside the graph (the graph then takes `z`,
//! `float32 [1, 4, n_fft/2, frames]` complex-as-channels, and returns the frequency-branch
//! spectrum too, as in the ONNX exports of `demucs.onnx`). The pipeline supports that variant via
//! `spec_input` / `spec_output` and [`crate::Spectral`], but it is exercised only with fake
//! models.

use crate::separator::StemKind;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct ModelContract {
    /// Rate the model works at (htdemucs: 44100).
    pub sample_rate: u32,
    /// Fixed segment length in samples (htdemucs: 7.8 s = 343 980).
    pub segment_samples: usize,
    /// Fraction of a segment shared with the next one.
    pub overlap: f32,
    pub n_fft: usize,
    pub hop: usize,
    /// Name of the waveform input.
    pub mix_input: String,
    /// Name of the spectrogram input, when the STFT is outside the graph.
    pub spec_input: Option<String>,
    /// Name of the waveform output `[1, 4, 2, N]`.
    pub stems_output: String,
    /// Name of the frequency-branch output `[1, 4, 4, n_fft/2, frames]`, to be inverted and added.
    pub spec_output: Option<String>,
    /// Source order of the model's stem axis. Demucs' own order is drums, bass, other, vocals.
    pub stem_order: [StemKind; 4],
}

impl ModelContract {
    /// The waveform-in, waveform-out variant (STFT inside the graph).
    pub fn htdemucs() -> Self {
        Self {
            sample_rate: 44_100,
            segment_samples: 343_980,
            overlap: 0.25,
            n_fft: 4096,
            hop: 1024,
            mix_input: "mix".into(),
            spec_input: None,
            stems_output: "stems".into(),
            spec_output: None,
            stem_order: [
                StemKind::Drums,
                StemKind::Bass,
                StemKind::Other,
                StemKind::Vocals,
            ],
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sample_rate == 0 || self.segment_samples < 2 {
            return Err("sampleRate and segmentSamples must be positive".into());
        }
        if !(0.0..0.9).contains(&self.overlap) {
            return Err("overlap must be in [0, 0.9)".into());
        }
        if self.spec_input.is_some() != self.spec_output.is_some() {
            return Err("specInput and specOutput go together".into());
        }
        if self.spec_input.is_some()
            && !(self.n_fft.is_power_of_two() && self.hop > 0 && self.n_fft % self.hop == 0)
        {
            return Err("nFft must be a power of two and a multiple of hop".into());
        }
        let mut seen = self.stem_order;
        seen.sort();
        if seen != StemKind::ALL {
            return Err("stemOrder must list each of the four stems once".into());
        }
        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "sampleRate": self.sample_rate,
            "segmentSamples": self.segment_samples,
            "overlap": self.overlap,
            "nFft": self.n_fft,
            "hop": self.hop,
            "mixInput": self.mix_input,
            "specInput": self.spec_input,
            "stemsOutput": self.stems_output,
            "specOutput": self.spec_output,
            "stemOrder": self.stem_order.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
        })
    }

    /// Fields that are absent take the [`Self::htdemucs`] value.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let d = Self::htdemucs();
        let text = |key: &str, default: &str| value[key].as_str().unwrap_or(default).to_owned();
        let opt = |key: &str| value[key].as_str().map(str::to_owned);
        let num = |key: &str, default: u64| value[key].as_u64().unwrap_or(default);
        let stem_order = match value["stemOrder"].as_array() {
            None => d.stem_order,
            Some(list) => {
                let kinds: Option<Vec<StemKind>> = list
                    .iter()
                    .map(|v| v.as_str().and_then(StemKind::from_name))
                    .collect();
                kinds
                    .and_then(|k| <[StemKind; 4]>::try_from(k).ok())
                    .ok_or("stemOrder must name four stems")?
            }
        };
        let contract = Self {
            sample_rate: num("sampleRate", d.sample_rate as u64) as u32,
            segment_samples: num("segmentSamples", d.segment_samples as u64) as usize,
            overlap: value["overlap"].as_f64().map_or(d.overlap, |v| v as f32),
            n_fft: num("nFft", d.n_fft as u64) as usize,
            hop: num("hop", d.hop as u64) as usize,
            mix_input: text("mixInput", &d.mix_input),
            spec_input: opt("specInput"),
            stems_output: text("stemsOutput", &d.stems_output),
            spec_output: opt("specOutput"),
            stem_order,
        };
        contract.validate()?;
        Ok(contract)
    }
}

impl Default for ModelContract {
    fn default() -> Self {
        Self::htdemucs()
    }
}
