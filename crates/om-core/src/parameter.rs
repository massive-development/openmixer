/// Single control surface for engine units: MIDI, UI and tests all drive parameters by string id.
pub trait ParameterTarget: Send + Sync {
    /// Returns `false` if the id is unknown. Safe to call from any non-audio thread.
    fn set_parameter(&self, id: &str, value: f64) -> bool;
}
