#![forbid(unsafe_code)]

//! MIDI input, controller maps and MIDI learn. Maps translate normalised events into string-id
//! parameter targets (`mixer.ch0.fader`, `deck.0.play`) through the shared `ParameterRouter`.
//! Hardware is optional: everything except [`MidiInputRouter::enable`] runs on injected bytes.

mod error;
mod event;
mod input;
mod learn;
mod map;
mod mapper;
mod message;
mod trace;
mod writer;

use std::sync::{Mutex, MutexGuard, PoisonError};

pub use error::MidiError;
pub use event::{MidiEvent, MidiKind};
pub use input::MidiInputRouter;
pub use learn::{LearnRequest, LearnType, MidiLearn};
pub use map::{MapMode, Mapping, MidiMap, MAP_VERSION};
pub use mapper::MidiMapper;
pub use message::{control_change, message_length, note_off, note_on, pitch_bend, program_change};
pub use trace::{parse_hex, replay_trace, MidiTrace, TimedMidi};

/// Locks a control-thread mutex, recovering from poisoning: every guarded value here is left
/// consistent by each critical section, so a panicked holder never leaves a half-written state.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
