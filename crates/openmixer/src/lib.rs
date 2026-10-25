#![forbid(unsafe_code)]

//! The app composition shared by the binary and the UI gate: engine, parameter router, theme
//! registry, preferences and the applier that pushes them into subsystems.

pub mod app_targets;
pub mod audio_settings;
pub mod automix_host;
pub mod automix_queue;
pub mod automix_service;
pub mod automix_settings;
pub mod error;
pub mod flags;

pub use error::AppError;
pub mod composition;
pub mod control_timer;
pub mod demo_map;
pub mod demo_scene;
pub mod engine_state;
pub mod library_smoke;
pub mod midi;
pub mod plugins;
pub mod record_service;
pub mod sampler_demo;
pub mod stems_host;
