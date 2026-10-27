#![forbid(unsafe_code)]

//! Audio-thread engine: FX rack, insert chains, mixer, decks, sync, sampler, recorder and output
//! routing. The device layer is in `om-audio`. Nothing here allocates, locks or does I/O in
//! `process` paths; buffers are sized in `prepare`.

#[cfg(all(test, debug_assertions))]
mod alloc_guard;

pub mod callback;
pub mod deck;
pub mod deck_control;
#[cfg(feature = "ffmpeg")]
pub mod decode_ffmpeg;
pub mod dj_engine;
pub mod fx;
pub mod fx_routing;
pub mod grid;
pub mod latency;
pub mod mixer;
pub mod output;
pub mod quantizer;
pub mod recorder;
pub mod sampler;
pub mod slip;
mod stage;
pub mod stem_control;
pub mod sync;

pub use callback::AudioCallback;
pub use deck_control::DeckControl;
pub use dj_engine::{DjEngine, DjEngineAudio};
pub use fx_routing::FxRouting;
pub use latency::{LatencyMonitor, LatencyStats};
pub use mixer::{ChannelInput, Mixer, MixerControl};
pub use output::{OutputPairs, OutputRouter, NO_PAIR};
pub use recorder::{
    RecordError, RecordFormat, RecordSettings, RecordState, RecordStatus, RecordSummary, Recorder,
    RecorderControl, RecorderTap,
};
pub use sampler::{PadMode, Sample, SampleRegistry, SamplerAudio, SamplerControl, SamplerError};
pub use stage::{ProcessStage, StageChain, StageInsert, INTERFACE_VERSION, MAX_STAGES};
pub use stem_control::{EngineCommands, StemSetRegistry};
pub use sync::{Sync, SyncMode};
