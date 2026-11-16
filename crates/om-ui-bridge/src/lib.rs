#![forbid(unsafe_code)]

//! Toolkit-free bridge between the engine and a web front end: binary snapshot packets out,
//! packed waveform peaks for the GPU. No window dependency, so it tests headless.

mod snapshot_packet;

pub use snapshot_packet::{pack_snapshot, packet_len, PACKET_MAGIC, PACKET_VERSION};

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
