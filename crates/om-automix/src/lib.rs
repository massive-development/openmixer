#![forbid(unsafe_code)]

//! Automix. Pure control-side code: nothing here touches the audio thread or the engine's
//! internals. A plan is a value; the controller applies it as typed `Command::Param` steps through
//! a [`om_core::CommandSink`] on the control tick, and the user's manual input always wins.
//!
//! Dependency direction: `openmixer` -> `om-automix` -> `om-core`. The engine's beat grid, the
//! analysis crate's sections and keys and the library are adapted by the host (see [`types`],
//! [`AutomixHost`], [`queue`]), which keeps this crate testable with synthetic grids, a fake host
//! and a fake sink.
//!
//! - [`planner`] / [`plan`]: `TransitionPlanner` turns two `DeckInfo`s into a `TransitionPlan`.
//! - [`controller`]: the `Idle -> Loading -> Armed -> Transitioning -> Cleanup` state machine.
//! - [`queue`]: the `AutomixQueue` trait and the pure selection rules behind it.
//! - [`stem_fade`]: stem ramps (volume, filter, mute) and the standard transition moves built from
//!   them (bass swap, vocals out, drums-only bridge, filter sweep).

pub mod controller;
#[cfg(test)]
mod fixtures;
mod moves;
pub mod plan;
pub mod planner;
pub mod queue;
pub mod readback;
pub mod stem_fade;
pub mod types;

pub use controller::{
    AutomixConfig, AutomixControl, AutomixController, AutomixHost, LoadPoll, NextTrack,
};
pub use plan::{PlanError, PlanOptions, Step, Style, TempoPlan, TransitionPlan};
pub use planner::{DefaultPlanner, TransitionPlanner};
pub use readback::{blank_snapshot, read_param};
pub use types::{Camelot, DeckInfo, FixedGrid, GridView, Section, SectionKind};
