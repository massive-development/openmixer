/// Read-only view of engine state by string id: the read-side twin of [`crate::ParameterTarget`].
/// Feeds controller LEDs and rings. Implementations read atomics or take short uncontended locks;
/// never call from the audio thread.
pub trait StateReader: Send + Sync {
    /// The current value for `id`, or `None` when the id is unknown.
    fn read_state(&self, id: &str) -> Option<f64>;
}
