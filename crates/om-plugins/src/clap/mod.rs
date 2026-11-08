//! CLAP plugin hosting through clack (ADR 0007, feature `clap`).
//!
//! Every clack and CLAP type stays inside this module behind our own types ([`ClapPlugin`],
//! [`StageDescriptor`], [`EventList`], [`ClapParam`]), so a breaking clack release, or a move to an
//! out-of-process host, touches only this module. The `unsafe` calls are in `entry.rs` (open a library), `gui_parent.rs` (parent a window)
//! and `fds.rs` (borrow a plugin's file descriptors), each with its `SAFETY` note. Unix only.
//!
//! Threads: each plugin instance has a control thread (`worker`) that owns clack's non-`Send`
//! instance; the audio side holds only the audio processor. Hosting is in-process: a plugin that
//! crashes natively takes the app with it, and `catch_unwind` around `process` is not a sandbox.

mod buffers;
mod discovery;
mod entry;
mod events;
mod fds;
mod gui;
mod gui_parent;
mod host;
mod loader;
mod param_bus;
mod params;
mod persist;
mod plugin;
mod ports;
mod probe;
mod stage;
mod state;
mod worker;

pub use discovery::{clap_files, ClapSearch, CLAP_EXTENSION};
pub use events::{EventList, HostEvent};
pub use gui::{choose_plan, GuiApi, GuiPlan, GuiSession, ParentWindow, PluginGui};
pub use loader::{ClapLoader, ClapPluginInfo};
pub use params::ClapParam;
pub use plugin::ClapPlugin;
pub use ports::{AudioPort, PortLayout, PortRole, StageDescriptor, StageRole};
pub use stage::ClapStage;
pub use state::{Fault, StageState};
