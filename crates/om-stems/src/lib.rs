#![forbid(unsafe_code)]

//! Stem separation: the streamed separator interface (v2), the htdemucs pipeline (resampling,
//! segmenting, overlap-add, STFT) around a [`StemInference`] network trait, the model manifest, and
//! licence-gated, checksum-verified weight downloads. The ONNX Runtime network (`OrtInference`) is
//! behind the off-by-default `onnx` feature; without it the pipeline runs on fakes and the
//! [`MockSeparator`] stands in.

mod cache;
mod contract;
mod lazy;
mod manifest;
mod model;
#[cfg(feature = "onnx")]
mod onnx;
mod pipeline;
mod resample;
mod separator;
mod service;
mod spectral;
mod wav;
mod weights;

pub use cache::{
    CacheError, StemCache, StemCacheSink, StemEntry, StemKey, StemMeta, StemReader,
    FORMAT as CACHE_FORMAT,
};
pub use contract::ModelContract;
pub use lazy::{LazySeparator, SeparatorFactory};
pub use manifest::{ManifestError, ModelManifest, HTDEMUCS_LICENSE_NAME, HTDEMUCS_LICENSE_TEXT};
pub use model::{InstallState, ModelInstaller};
#[cfg(feature = "onnx")]
pub use onnx::{default_threads, OrtInference};
pub use pipeline::{
    HtDemucsPipeline, HtDemucsSeparator, SegmentInput, SegmentOutput, StemInference, EMIT_CHUNK,
    MIN_STD,
};
pub use resample::StreamResampler;
pub use separator::{
    FileStereoSource, MockSeparator, SliceSource, StemBuffer, StemError, StemKind, StemSeparator,
    StemSet, StemSetSink, StemSink, StereoSource, INTERFACE_VERSION,
};
pub use service::{
    clear_stem_cache, ModelGate, StemRequest, StemService, StemServiceConfig, StemServiceError,
};
pub use spectral::Spectral;
pub use weights::{Downloader, LicensePrompt, UrlDownloader, WeightError, WeightManager};
