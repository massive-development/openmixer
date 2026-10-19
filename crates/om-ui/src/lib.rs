#![forbid(unsafe_code)]

//! UI logic for OpenMixer, without a windowing toolkit: themes, value formatting, waveform and
//! meter geometry, and the view-model state of the deck and mixer widgets. Every widget reads the
//! engine and writes through the string-id control surface, so it can be driven and tested headless.
//! The gpui front end (Todo 8b) renders these models.

pub mod theme;
