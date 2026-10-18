#![forbid(unsafe_code)]

//! Audio-thread engine: FX rack now; decks, channel strips and the mixer follow. Nothing here
//! allocates, locks or does I/O in `process` paths; buffers are sized in `prepare`.

pub mod fx;
mod stage;

pub use stage::ProcessStage;
