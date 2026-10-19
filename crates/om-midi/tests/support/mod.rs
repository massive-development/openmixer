//! Shared test helpers: a recording parameter target and the controller fixtures.

#![allow(dead_code)]

use om_core::ParameterTarget;
use std::path::PathBuf;
use std::sync::Mutex;

/// One `set_parameter` call seen by a [`RecordingTarget`].
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub id: String,
    pub value: f64,
}

/// Records every change; the test-side stand-in for the engine.
#[derive(Default)]
pub struct RecordingTarget {
    changes: Mutex<Vec<Change>>,
}

impl RecordingTarget {
    pub fn changes(&self) -> Vec<Change> {
        self.changes.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.changes.lock().unwrap().clear();
    }

    /// Latest value sent to `id`, or 0.0 when none was.
    pub fn last(&self, id: &str) -> f64 {
        self.last_or(id, 0.0)
    }

    pub fn last_or(&self, id: &str, fallback: f64) -> f64 {
        self.changes
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|c| c.id == id)
            .map_or(fallback, |c| c.value)
    }

    pub fn count(&self, id: &str) -> usize {
        self.changes
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.id == id)
            .count()
    }
}

impl ParameterTarget for RecordingTarget {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        self.changes.lock().unwrap().push(Change {
            id: id.to_owned(),
            value,
        });
        true
    }
}

/// `fixtures/controllers` at the workspace root.
pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/controllers")
}

pub fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "{actual} is not {expected}"
    );
}

/// Test double for the output side: keeps every message in order.
#[derive(Default)]
pub struct RecordingOutputSink {
    sent: Mutex<Vec<Vec<u8>>>,
}

impl RecordingOutputSink {
    pub fn sent(&self) -> Vec<Vec<u8>> {
        self.sent.lock().unwrap().clone()
    }

    pub fn last(&self) -> Option<Vec<u8>> {
        self.sent.lock().unwrap().last().cloned()
    }
}

impl om_midi::MidiOutputSink for RecordingOutputSink {
    fn send(&self, bytes: &[u8]) {
        self.sent.lock().unwrap().push(bytes.to_vec());
    }
}

/// A state source with settable values; an id that was never set is unknown.
#[derive(Default)]
pub struct FakeState {
    values: Mutex<std::collections::HashMap<String, f64>>,
}

impl FakeState {
    pub fn set(&self, id: &str, value: f64) {
        self.values.lock().unwrap().insert(id.to_owned(), value);
    }
}

impl om_core::StateReader for FakeState {
    fn read_state(&self, id: &str) -> Option<f64> {
        self.values.lock().unwrap().get(id).copied()
    }
}
