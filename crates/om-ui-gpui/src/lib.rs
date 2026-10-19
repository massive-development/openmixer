#![forbid(unsafe_code)]

//! GPUI window layer for OpenMixer. Drawing and event glue only; UI logic lives in `om-ui`.

pub mod app;
pub mod controls;
pub mod debug_panel;
pub mod deck_view;
pub mod drag;
pub mod mixer_view;
pub mod paint;
pub mod settings_view;
