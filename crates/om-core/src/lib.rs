#![forbid(unsafe_code)]

//! Control surface, parameter routing and lock-free frame transport shared by the engine,
//! the UI and MIDI. No audio processing lives here.

pub mod atomic;
mod frame_ring;
mod parameter;
mod ring;
mod router;
mod state;
pub mod sync;

pub use frame_ring::FrameRing;
pub use parameter::ParameterTarget;
pub use ring::{spsc, SpscConsumer, SpscProducer};
pub use router::ParameterRouter;
pub use state::StateReader;

/// The OpenMixer crate version, taken from the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
