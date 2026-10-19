//! Output side of the device layer: the open controller output ports and the rule that pairs
//! them with the enabled inputs. Feedback reaches controllers through [`MidiOutputSink`].
//! Port of `src/controllers/MidiOutputRouter.*` and `MidiOutputSink.h`.

use crate::backend::{MidiBackend, MidiOutputPort, MidiPortInfo};
use crate::error::MidiError;
use crate::lock;
use std::sync::Mutex;

/// Where feedback messages go. `send` is called from the control thread.
pub trait MidiOutputSink: Send + Sync {
    fn send(&self, bytes: &[u8]);
}

/// Output ports whose name matches an enabled input's name (every output when `all`). A
/// controller's input and output usually share a name, so this pairs them without any extra setup.
pub fn pair_outputs(
    inputs: &[MidiPortInfo],
    enabled_ids: &[String],
    all: bool,
    outputs: &[MidiPortInfo],
) -> Vec<MidiPortInfo> {
    let names: Vec<&str> = inputs
        .iter()
        .filter(|p| all || enabled_ids.iter().any(|id| *id == p.id))
        .map(|p| p.name.as_str())
        .collect();
    outputs
        .iter()
        .filter(|p| names.contains(&p.name.as_str()))
        .cloned()
        .collect()
}

/// Holds the open output ports and fans each message out to all of them.
#[derive(Default)]
pub struct MidiOutputRouter {
    ports: Mutex<Vec<Box<dyn MidiOutputPort>>>,
}

impl MidiOutputRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the open ports with the outputs paired to these inputs. Returns how many opened;
    /// a port that fails to open is skipped, so one dead controller does not stop the others.
    pub fn open_paired_with(
        &self,
        backend: &mut dyn MidiBackend,
        enabled_ids: &[String],
        all: bool,
    ) -> Result<usize, MidiError> {
        self.close_all();
        let inputs = backend.inputs()?;
        let outputs = backend.outputs()?;
        let mut opened = Vec::new();
        for port in pair_outputs(&inputs, enabled_ids, all, &outputs) {
            if let Ok(out) = backend.open_output(&port.id) {
                opened.push(out);
            }
        }
        let count = opened.len();
        *lock(&self.ports) = opened;
        Ok(count)
    }

    pub fn close_all(&self) {
        lock(&self.ports).clear();
    }

    pub fn num_open(&self) -> usize {
        lock(&self.ports).len()
    }
}

impl MidiOutputSink for MidiOutputRouter {
    fn send(&self, bytes: &[u8]) {
        for port in lock(&self.ports).iter_mut() {
            // A port that cannot write is left alone: the others still get the message.
            let _ = port.send(bytes);
        }
    }
}
