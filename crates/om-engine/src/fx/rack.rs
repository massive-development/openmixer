//! Series chain of FX units sharing one tempo; plugs into a mixer insert via `ProcessStage`.
//!
//! Threads: build and `set_parameter`/`set_tempo` on the control thread; `process` on the audio
//! thread.

use super::factory::FxFactory;
use super::unit::FxUnit;
use crate::ProcessStage;
use om_core::ParameterTarget;

pub struct FxRack {
    units: Vec<FxUnit>,
}

impl FxRack {
    /// One unit per known effect name, in order; unknown names are skipped.
    pub fn new(layout: &[&str], factory: &FxFactory) -> Self {
        let units = layout
            .iter()
            .filter_map(|name| factory.create(name))
            .map(FxUnit::new)
            .collect();
        Self { units }
    }

    pub fn set_tempo(&self, bpm: f64) {
        for unit in &self.units {
            unit.set_tempo(bpm);
        }
    }

    /// Follow a deck's grid tempo: `grid_bpm * playback_rate`. Call from the control thread
    /// whenever the grid or rate changes.
    pub fn set_tempo_from_grid_bpm(&self, grid_bpm: f64, playback_rate: f64) {
        self.set_tempo(grid_bpm * playback_rate);
    }

    pub fn num_units(&self) -> usize {
        self.units.len()
    }

    pub fn unit(&self, index: usize) -> Option<&FxUnit> {
        self.units.get(index)
    }

    pub fn unit_mut(&mut self, index: usize) -> Option<&mut FxUnit> {
        self.units.get_mut(index)
    }

    /// Index of the first unit running the named effect.
    pub fn find_unit(&self, effect_name: &str) -> Option<usize> {
        self.units
            .iter()
            .position(|u| u.effect().name() == effect_name)
    }
}

impl Default for FxRack {
    fn default() -> Self {
        Self::new(FxFactory::builtin_names(), &FxFactory::with_built_ins())
    }
}

/// Parses `fx<N>.<rest>` with 1 to 3 digits; returns `(N, rest)`.
fn parse_unit_id(id: &str) -> Option<(usize, &str)> {
    let rest = id.strip_prefix("fx")?;
    let dot = rest.find('.')?;
    let digits = &rest[..dot];
    if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((digits.parse().ok()?, &rest[dot + 1..]))
}

impl ParameterTarget for FxRack {
    /// Ids: "bpm", "fx<N>.enable", "fx<N>.mix", "fx<N>.<effect param>" (N = unit index).
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        if id == "bpm" {
            self.set_tempo(value);
            return true;
        }
        match parse_unit_id(id) {
            Some((index, unit_id)) => self
                .unit(index)
                .is_some_and(|unit| unit.set_parameter(unit_id, value)),
            None => false,
        }
    }
}

impl ProcessStage for FxRack {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        for unit in &mut self.units {
            unit.prepare(sample_rate, max_block_size);
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for unit in &mut self.units {
            unit.process(left, right);
        }
    }
}
