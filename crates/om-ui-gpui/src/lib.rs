#![forbid(unsafe_code)]

//! GPUI window layer for OpenMixer. Drawing and event glue only; UI logic lives in `om-ui`.

pub mod app;
pub mod automix_bar;
pub mod controls;
pub mod debug_panel;
pub mod deck_view;
pub mod drag;
pub mod library;
pub mod library_view;
pub mod mixer_view;
pub mod paint;
pub mod sampler_view;
pub mod settings_view;
pub mod text_entry;
