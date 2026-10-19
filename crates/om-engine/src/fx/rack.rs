//! Series chain of FX units sharing one tempo; plugs into a mixer insert via `ProcessStage`.
//!
//! Threads: `set_parameter` and the tempo setters work from any thread through an
//! [`FxRackControl`] (atomics); `prepare` and `process` run on the audio side that owns the rack.

use super::factory::FxFactory;
use super::unit::{FxUnit, FxUnitControl};
use crate::grid::BeatGrid;
use crate::ProcessStage;
use om_core::param_id::{FxParam, FxRackParam};
use om_core::ParameterTarget;

pub struct FxRack {
    units: Vec<FxUnit>,
    control: FxRackControl,
}

impl FxRack {
    /// One unit per known effect name, in order; unknown names are skipped.
    pub fn new(layout: &[&str], factory: &FxFactory) -> Self {
        let units: Vec<FxUnit> = layout
            .iter()
            .filter_map(|name| factory.create(name))
            .map(FxUnit::new)
            .collect();
        let control = FxRackControl::new(&units);
        Self { units, control }
    }

    /// Control-thread handle; keeps working after the rack has been moved into a mixer insert.
    pub fn control(&self) -> FxRackControl {
        self.control.clone()
    }

    pub fn set_tempo(&self, bpm: f64) {
        self.control.set_tempo(bpm);
    }

    /// Follow a deck's grid tempo: `grid_bpm * playback_rate`. Call from the control thread
    /// whenever the grid or rate changes.
    pub fn set_tempo_from_grid_bpm(&self, grid_bpm: f64, playback_rate: f64) {
        self.control
            .set_tempo_from_grid_bpm(grid_bpm, playback_rate);
    }

    /// Follow a beat grid: its tempo times `playback_rate`. Control thread.
    pub fn set_tempo_from_grid(&self, grid: &dyn BeatGrid, playback_rate: f64) {
        self.control
            .set_tempo_from_grid_bpm(grid.bpm(), playback_rate);
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
        self.control.find_unit(effect_name)
    }
}

impl Default for FxRack {
    fn default() -> Self {
        Self::new(FxFactory::builtin_names(), &FxFactory::with_built_ins())
    }
}

/// Control-thread view of an [`FxRack`]: the same units, addressed by index and id.
#[derive(Debug, Clone)]
pub struct FxRackControl {
    units: Vec<FxUnitControl>,
}

impl FxRackControl {
    fn new(units: &[FxUnit]) -> Self {
        Self {
            units: units.iter().map(FxUnit::control).collect(),
        }
    }

    pub fn set_tempo(&self, bpm: f64) {
        for unit in &self.units {
            unit.set_tempo(bpm);
        }
    }

    /// See [`FxRack::set_tempo_from_grid_bpm`].
    pub fn set_tempo_from_grid_bpm(&self, grid_bpm: f64, playback_rate: f64) {
        self.set_tempo(grid_bpm * playback_rate);
    }

    /// See [`FxRack::set_tempo_from_grid`].
    pub fn set_tempo_from_grid(&self, grid: &dyn BeatGrid, playback_rate: f64) {
        self.set_tempo(grid.bpm() * playback_rate);
    }

    pub fn num_units(&self) -> usize {
        self.units.len()
    }

    pub fn unit(&self, index: usize) -> Option<&FxUnitControl> {
        self.units.get(index)
    }

    /// Index of the first unit running the named effect.
    pub fn find_unit(&self, effect_name: &str) -> Option<usize> {
        self.units
            .iter()
            .position(|u| u.effect_name() == effect_name)
    }
}

impl FxRackControl {
    /// Applies a rack parameter: the rack tempo (`unit` is `None`, `Bpm`) or a parameter of unit
    /// `unit`. Returns `false` for a missing unit or a parameter the rack does not take.
    pub fn set_fx(&self, unit: Option<u16>, p: &FxParam, value: f64) -> bool {
        match (unit, p) {
            (None, FxParam::Bpm) => {
                self.set_tempo(value);
                true
            }
            (None, _) => false,
            (Some(index), p) => self
                .unit(usize::from(index))
                .is_some_and(|unit| unit.set_fx_param(p, value)),
        }
    }
}

impl ParameterTarget for FxRackControl {
    /// The rack's id: "bpm", or "fx<N>.<param>" with N the unit index.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match id.parse::<FxRackParam>() {
            Ok(FxRackParam { unit, p }) => self.set_fx(unit, &p, value),
            Err(_) => false,
        }
    }
}

impl ParameterTarget for FxRack {
    /// Same ids as [`FxRackControl`].
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        self.control.set_parameter(id, value)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::FixedTempoGrid;

    /// Tempo from an id and a beat grid.
    #[test]
    fn tempo_from_id_and_beat_grid() {
        let rack = FxRack::default();
        assert!(rack.control().set_parameter("bpm", 128.0));
        assert!((rack.unit(3).unwrap().tempo() - 128.0).abs() < 1e-9);
        let grid = FixedTempoGrid::new(100.0, 48_000.0, 0);
        rack.control().set_tempo_from_grid(&grid, 1.08);
        for i in 0..rack.num_units() {
            assert!((rack.unit(i).unwrap().tempo() - 108.0).abs() < 1e-9);
        }
    }
}
