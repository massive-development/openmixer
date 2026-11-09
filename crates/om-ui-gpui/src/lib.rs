#![forbid(unsafe_code)]

//! GPUI window layer for OpenMixer. Drawing and event glue only; UI logic lives in `om-ui`. Built on
//! `gpui-pre` (imported as `gpui`) and `gpui-kit`; standard widgets come from `gpui-component`,
//! themed by `theme_bridge`, and knobs, faders, meters and waveforms are painted here. The control
//! parts and their states are in `parts` and `controls`; `gallery` shows them all.

pub mod app;
pub mod automix_bar;
pub mod controls;
pub mod debug_panel;
pub mod deck_view;
pub mod desktop;
pub mod drag;
pub mod fader_paint;
pub mod font;
pub mod frame_stats;
pub mod gallery;
pub mod header_view;
pub mod knob_paint;
pub mod library;
pub mod library_view;
pub mod loudness_paint;
pub mod media_keys;
pub mod meter_paint;
pub mod mixer_view;
pub mod paint;
pub mod parts;
pub mod platter_paint;
mod plugin_window;
pub mod record_button;
pub mod sampler_view;
pub mod settings_view;
pub mod stage_view;
pub mod text_entry;
pub mod theme_bridge;
pub mod waveform_paint;
pub mod window_size;

#[cfg(test)]
mod headless_tests;
