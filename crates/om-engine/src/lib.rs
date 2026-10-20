#![forbid(unsafe_code)]

//! Audio-thread engine: FX rack, insert chains, mixer and output routing. Decks, the device layer
//! and sync follow. Nothing here allocates, locks or does I/O in `process` paths; buffers are sized
//! in `prepare`.

#[cfg(all(test, debug_assertions))]
mod alloc_guard;

pub mod callback;
pub mod deck;
pub mod deck_control;
pub mod dj_engine;
pub mod fx;
pub mod fx_routing;
pub mod grid;
pub mod latency;
pub mod mixer;
pub mod output;
pub mod quantizer;
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
pub use stage::{ProcessStage, StageChain, StageInsert, INTERFACE_VERSION, MAX_STAGES};
pub use stem_control::{EngineCommands, StemSetRegistry};
pub use sync::{Sync, SyncMode};
