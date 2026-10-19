#![forbid(unsafe_code)]

//! Stem separation: the streamed separator interface (v2), the htdemucs pipeline (resampling,
//! segmenting, overlap-add, STFT) around a [`StemInference`] network trait, the model manifest, and
//! licence-gated, checksum-verified weight downloads. The ONNX Runtime network (`OrtSeparator`) is
//! behind the off-by-default `onnx` feature; without it the pipeline runs on fakes and the
//! [`MockSeparator`] stands in.

mod contract;
mod manifest;
#[cfg(feature = "onnx")]
mod onnx;
mod pipeline;
mod resample;
mod separator;
mod spectral;
mod weights;

pub use contract::ModelContract;
pub use manifest::{ManifestError, ModelManifest};
#[cfg(feature = "onnx")]
pub use onnx::OrtInference;
pub use pipeline::{
    HtDemucsPipeline, HtDemucsSeparator, SegmentInput, SegmentOutput, StemInference, EMIT_CHUNK,
    MIN_STD,
};
pub use resample::StreamResampler;
pub use separator::{
    FileStereoSource, MockSeparator, SliceSource, StemBuffer, StemError, StemKind, StemSeparator,
    StemSet, StemSetSink, StemSink, StereoSource, INTERFACE_VERSION,
};
pub use spectral::Spectral;
pub use weights::{Downloader, LicensePrompt, UrlDownloader, WeightError, WeightManager};
