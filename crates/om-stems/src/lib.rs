#![forbid(unsafe_code)]

//! Stem separation: the separator interface, the model manifest, and licence-gated, checksum-verified
//! weight downloads. The ONNX Runtime separator is open work: no ONNX
//! Runtime binding is linked yet, so the [`MockSeparator`] stands in for it.

mod manifest;
mod separator;
mod weights;

pub use manifest::{ManifestError, ModelManifest};
pub use separator::{MockSeparator, StemKind, StemSeparator, StemSet, INTERFACE_VERSION};
pub use weights::{Downloader, LicensePrompt, UrlDownloader, WeightError, WeightManager};
