#![forbid(unsafe_code)]

//! Test support shared by the openmixer crates. Dev-dependency only: no shipped crate may depend on
//! this one as a normal dependency.
//!
//! - [`wav`]: float and 16-bit PCM WAV fixtures, a reader for round trips, interleave helpers.
//! - [`signals`]: deterministic sines, noise and impulse trains.
//! - [`level`]: gain/dB conversion and peak/RMS measures.
//! - [`wait`]: condition polling with a timeout, and the `assert_eventually!` macro.
//! - [`fs`]: a temporary directory that is removed on drop.

pub mod fs;
pub mod level;
pub mod signals;
pub mod wait;
pub mod wav;

#[cfg(test)]
mod tests;
