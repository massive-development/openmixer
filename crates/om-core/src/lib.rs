#![forbid(unsafe_code)]

//! Control surface, parameter routing and lock-free frame transport shared by the engine,
//! the UI and MIDI. No audio processing lives here.

pub mod atomic;
pub mod command;
mod frame_ring;
pub mod jobs;
pub mod param_id;
mod parameter;
mod router;
mod snapshot;
mod state;
pub mod state_id;
pub mod sync;

pub use command::{Command, CommandError, CommandSink, SampleId, StemSetRef};
pub use frame_ring::FrameRing;
pub use param_id::{Namespace, ParamError, ParamId};
pub use parameter::{ParamTarget, ParameterTarget};
pub use router::{AutomixSink, InputObserver, ParameterRouter};
pub use snapshot::{
    AutomixSnapshot, AutomixState, AutomixStyle, ChannelSnapshot, DeckSnapshot, EngineSnapshot,
    MixerSnapshot, PadMode, PadSnapshot, SamplerSnapshot, StemSnapshot, XfaderAssign,
};
pub use state::{StateKey, StateReader};
pub use state_id::StateId;

/// The OpenMixer crate version, taken from the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
