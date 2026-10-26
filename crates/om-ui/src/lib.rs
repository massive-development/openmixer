#![forbid(unsafe_code)]

//! UI logic for OpenMixer, without a windowing toolkit: themes, value formatting, waveform and
//! meter geometry, and the view-model state of the deck and mixer widgets. Every widget reads the
//! engine and writes through the string-id control surface, so it can be driven and tested headless.
//! The gpui front end renders these models.

pub mod automix_bar;
pub mod binding;
pub mod control;
pub mod format;
pub mod geometry;
pub mod header_layout;
pub mod labels;
pub mod library;
pub mod main_layout;
pub mod mapping_editor;
pub mod meter;
pub mod preferences;
pub mod preferences_applier;
pub mod record_bar;
pub mod sample_bank;
pub mod sampler_binding;
pub mod settings;
pub mod sidebar;
pub mod stem_binding;
pub mod stems;
pub mod style;
pub mod theme;
pub mod waveform;
pub mod widgets;
