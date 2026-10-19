use crate::state_id::StateId;

/// A state id prepared for repeated reads: its text, plus the parsed [`StateId`] when the text names
/// engine state. Build it once (when a controller map loads) and read it on every pass.
#[derive(Debug, Clone, PartialEq)]
pub struct StateKey {
    id: String,
    state: Option<StateId>,
}

impl StateKey {
    /// Parses `id` once. Ids the engine does not own keep their text and read through the string
    /// path.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            state: id.parse().ok(),
        }
    }

    /// The id as written.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// The parsed engine id, or `None` when the text is not an engine state id.
    pub fn state_id(&self) -> Option<&StateId> {
        self.state.as_ref()
    }
}

/// Read-only view of engine state by string id: the read-side twin of [`crate::ParameterTarget`].
/// Feeds controller LEDs and rings. Implementations read atomics or take short uncontended locks;
/// never call from the audio thread.
pub trait StateReader: Send + Sync {
    /// The current value for `id`, or `None` when the id is unknown.
    fn read_state(&self, id: &str) -> Option<f64>;

    /// Reads one pass of prepared keys into `out` (one slot per key; extra slots are left alone).
    /// Readers that take a snapshot override this so every key in the pass reads the same capture.
    /// The default reads each key with [`StateReader::read_state`].
    fn read_states(&self, keys: &[StateKey], out: &mut [Option<f64>]) {
        for (key, slot) in keys.iter().zip(out.iter_mut()) {
            *slot = self.read_state(key.as_str());
        }
    }
}
