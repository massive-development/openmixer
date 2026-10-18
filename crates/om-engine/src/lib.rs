#![forbid(unsafe_code)]

//! Audio-thread engine: FX rack, insert chains, mixer and output routing. Decks, the device layer
//! and sync follow. Nothing here allocates, locks or does I/O in `process` paths; buffers are sized
//! in `prepare`.

pub mod callback;
pub mod fx;
pub mod fx_routing;
pub mod latency;
pub mod mixer;
pub mod output;
mod stage;

pub use callback::AudioCallback;
pub use fx_routing::FxRouting;
pub use latency::{LatencyMonitor, LatencyStats};
pub use mixer::{ChannelInput, Mixer, MixerControl};
pub use output::{OutputPairs, OutputRouter, NO_PAIR};
pub use stage::{ProcessStage, StageChain, StageInsert, INTERFACE_VERSION, MAX_STAGES};
