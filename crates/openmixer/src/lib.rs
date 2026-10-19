#![forbid(unsafe_code)]

//! The app composition shared by the binary and the UI gate: engine, parameter router, theme
//! registry, preferences and the applier that pushes them into subsystems. Port of `src/app/`.

pub mod app_targets;
pub mod audio_settings;
pub mod composition;
pub mod engine_state;
pub mod library_smoke;
pub mod midi;
pub mod plugins;
