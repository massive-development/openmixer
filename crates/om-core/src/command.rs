//! Typed control commands. A [`Command`] is what a control surface asks the engine to do; the
//! parameter variant is the one MIDI, the UI and tests use today. The other variants carry payloads
//! that later phases fill in (stems, sampler, automix), so they are placeholders for now.
//!
//! Threads: sending is a control-thread operation, like [`crate::ParameterTarget`].

use crate::param_id::{AutomixParam, ParamError};
use crate::ParamId;
use thiserror::Error;

/// Reference to a stem set (placeholder until stems land).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StemSetRef(pub u64);

/// Reference to a decoded sampler sample; the engine's `SampleRegistry` maps it to the sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleId(pub u64);

/// An automix control: one of the `automix.*` parameters with its value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutomixCommand {
    pub p: AutomixParam,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Set one parameter. MIDI, UI and tests all reach the engine through this variant.
    Param {
        id: ParamId,
        value: f64,
    },
    LoadStems {
        deck: u8,
        set: StemSetRef,
    },
    SamplerAssign {
        pad: u8,
        sample: SampleId,
    },
    Automix(AutomixCommand),
}

impl Command {
    /// A `Command::Param` for `id`. Fails for an id that does not validate (see
    /// [`ParamId::validate`]), so a control surface cannot send one the engine would print wrongly.
    pub fn param(id: ParamId, value: f64) -> Result<Self, ParamError> {
        id.validate()?;
        Ok(Self::Param { id, value })
    }
}

/// Why a command was not carried out.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum CommandError {
    /// The sink no longer accepts commands (the engine has shut down).
    #[error("command sink is closed")]
    Closed,
    /// The sink does not handle this command.
    #[error("command rejected: {0}")]
    Rejected(String),
}

/// Accepts commands from a control surface. Safe to call from any non-audio thread.
pub trait CommandSink: Send + Sync {
    fn send(&self, cmd: Command) -> Result<(), CommandError>;
}
