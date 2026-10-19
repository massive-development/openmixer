#![forbid(unsafe_code)]

//! MIDI input, controller maps, MIDI learn and controller feedback. Maps translate normalised
//! events into string-id parameter targets (`mixer.ch0.fader`, `deck.0.play`) through the shared
//! `ParameterRouter`, and feedback rules light controller LEDs from engine state.
//! Hardware is optional: the device layer is a [`MidiBackend`]; tests use [`MockBackend`] and feed
//! bytes straight into the mapper.

#[cfg(target_os = "linux")]
mod alsa_backend;
mod backend;
mod echo;
mod error;
mod event;
mod feedback;
mod input;
mod learn;
mod map;
mod mapper;
mod message;
mod mock;
mod output_router;
mod output_rule;
mod trace;
mod writer;

use om_core::sync::LockExt;
use std::sync::{Mutex, MutexGuard};

#[cfg(target_os = "linux")]
pub use alsa_backend::AlsaBackend;
pub use backend::{
    system_backend, MidiBackend, MidiCallback, MidiInputConnection, MidiOutputPort, MidiPortInfo,
    PortSnapshot,
};
pub use echo::{EchoGuard, ECHO_WINDOW};
pub use error::MidiError;
pub use event::{MidiEvent, MidiKind};
pub use feedback::{system_clock, Clock, MidiFeedback};
pub use input::{EnableReport, MidiInputRouter};
pub use learn::{LearnRequest, LearnType, MidiLearn};
pub use map::{MapMode, Mapping, MidiMap, MAP_VERSION};
pub use mapper::MidiMapper;
pub use message::{control_change, message_length, note_off, note_on, pitch_bend, program_change};
pub use mock::MockBackend;
pub use output_router::{pair_outputs, MidiOutputRouter, MidiOutputSink};
pub use output_rule::{Blink, OutputKind, OutputMode, OutputRule};
pub use trace::{parse_hex, replay_trace, MidiTrace, TimedMidi};

/// Locks a control-thread mutex, recovering from poisoning: every guarded value here is left
/// consistent by each critical section, so a panicked holder never leaves a half-written state.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock_unpoisoned()
}
