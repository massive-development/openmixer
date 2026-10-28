#![forbid(unsafe_code)]

//! GPUI window layer for OpenMixer. Drawing and event glue only; UI logic lives in `om-ui`.

pub mod app;
pub mod automix_bar;
pub mod controls;
pub mod debug_panel;
pub mod deck_view;
pub mod drag;
pub mod fader_paint;
pub mod font;
pub mod frame_stats;
pub mod header_view;
pub mod knob_paint;
pub mod library;
pub mod library_view;
pub mod meter_paint;
pub mod mixer_view;
pub mod paint;
pub mod platter_paint;
pub mod record_button;
pub mod sampler_view;
pub mod settings_view;
pub mod stage_view;
pub mod text_entry;
pub mod waveform_paint;
pub mod window_size;

#[cfg(test)]
mod headless_tests;
