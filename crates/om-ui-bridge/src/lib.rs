#![forbid(unsafe_code)]

//! Toolkit-free bridge between the engine and a web front end: binary snapshot packets out,
//! packed waveform peaks for the GPU. No window dependency, so it tests headless.

mod control;
mod control_spec;
mod peaks;
mod snapshot_packet;
mod theme_css;

pub use control::{dispatch_control, slice_control_ids};
pub use control_spec::{slice_controls, ControlKind, ControlSpec};
pub use peaks::{
    pack_peaks, peak_levels, PeakBlob, PeakLevel, BYTES_PER_POINT, FILL, MIN_HALF_PX,
    PLAYED_BRIGHTNESS,
};
pub use snapshot_packet::{pack_snapshot, packet_len, PACKET_MAGIC, PACKET_VERSION};
pub use theme_css::theme_css;

/// Why the bridge refused a request from the front end.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BridgeError {
    #[error("unknown control id `{0}`")]
    UnknownControl(String),
    #[error("control value is not a finite number")]
    NonFinite,
    #[error("control rejected: {0}")]
    Rejected(String),
}
