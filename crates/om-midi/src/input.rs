//! Routes live MIDI bytes into a [`MidiMapper`], with optional recording. Opening the inputs goes
//! through a [`MidiBackend`]; the routing itself runs on injected bytes too, so tests need no device.

use crate::backend::{MidiBackend, MidiInputConnection, MidiPortInfo};
use crate::error::MidiError;
use crate::lock;
use crate::mapper::MidiMapper;
use crate::trace::MidiTrace;
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Recording {
    started: Instant,
    trace: MidiTrace,
}

struct Shared {
    mapper: Arc<MidiMapper>,
    recording: Mutex<Option<Recording>>,
}

impl Shared {
    fn handle(&self, bytes: &[u8]) -> usize {
        if let Some(rec) = lock(&self.recording).as_mut() {
            // Debug use only: allocates.
            rec.trace
                .add(rec.started.elapsed().as_secs_f64(), bytes.to_vec());
        }
        self.mapper.handle_message(bytes)
    }
}

/// The inputs the user selected: every input, or the listed ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Selection {
    all: bool,
    ids: Vec<String>,
}

impl Selection {
    fn selects(&self, port: &MidiPortInfo) -> bool {
        self.all || self.ids.contains(&port.id)
    }
}

#[derive(Default)]
struct Open {
    selection: Selection,
    connections: Vec<(String, Box<dyn MidiInputConnection>)>,
}

/// What [`MidiInputRouter::enable`] did: how many inputs opened, and why the others did not.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnableReport {
    pub opened: usize,
    pub failed: Vec<String>,
}

/// Routes live MIDI bytes into a [`MidiMapper`] and records them while recording. Selection and
/// hot-plug run on the control thread; the input callbacks run on the backend's threads.
pub struct MidiInputRouter {
    shared: Arc<Shared>,
    open: Mutex<Open>,
}

impl MidiInputRouter {
    pub fn new(mapper: Arc<MidiMapper>) -> Self {
        Self {
            shared: Arc::new(Shared {
                mapper,
                recording: Mutex::new(None),
            }),
            open: Mutex::new(Open::default()),
        }
    }

    /// While recording, incoming messages are also stored in a trace.
    pub fn start_recording(&self) {
        *lock(&self.shared.recording) = Some(Recording {
            started: Instant::now(),
            trace: MidiTrace::default(),
        });
    }

    /// Stops recording and returns the trace, if one was running.
    pub fn stop_recording(&self) -> Option<MidiTrace> {
        lock(&self.shared.recording).take().map(|r| r.trace)
    }

    /// The input callback body: records (when recording) and routes one raw message through the
    /// mapper. Returns the number of parameter updates produced.
    pub fn handle_incoming(&self, bytes: &[u8]) -> usize {
        self.shared.handle(bytes)
    }

    /// Replaces the selection: closes every open input, then opens the selected ones
    /// (every input when `all`). A port that cannot open is reported and skipped.
    pub fn enable(
        &self,
        backend: &mut dyn MidiBackend,
        all: bool,
        ids: &[String],
    ) -> Result<EnableReport, MidiError> {
        let mut open = lock(&self.open);
        open.connections.clear();
        open.selection = Selection {
            all,
            ids: ids.to_vec(),
        };
        let inputs = backend.inputs()?;
        let selection = open.selection.clone();
        let mut report = EnableReport::default();
        for port in inputs.iter().filter(|p| selection.selects(p)) {
            match self.open_one(backend, &port.id) {
                Ok(connection) => {
                    open.connections.push((port.id.clone(), connection));
                    report.opened += 1;
                }
                Err(error) => report.failed.push(format!("{}: {error}", port.name)),
            }
        }
        Ok(report)
    }

    /// Closes every input. The selection is kept, so [`MidiInputRouter::refresh`] stays quiet.
    pub fn disable_all(&self) {
        lock(&self.open).connections.clear();
    }

    /// Ids of the inputs that are open now.
    pub fn open_ids(&self) -> Vec<String> {
        lock(&self.open)
            .connections
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Hot-plug: when the backend's port list changed, closes inputs whose device is gone and opens
    /// selected inputs that appeared. Returns true when anything changed.
    pub fn refresh(&self, backend: &mut dyn MidiBackend) -> Result<bool, MidiError> {
        if !backend.refresh()? {
            return Ok(false);
        }
        let inputs = backend.inputs()?;
        let mut open = lock(&self.open);
        let selection = open.selection.clone();
        let before = open.connections.len();
        open.connections
            .retain(|(id, _)| inputs.iter().any(|p| p.id == *id));
        let mut changed = open.connections.len() != before;
        for port in inputs.iter().filter(|p| selection.selects(p)) {
            if open.connections.iter().any(|(id, _)| *id == port.id) {
                continue;
            }
            if let Ok(connection) = self.open_one(backend, &port.id) {
                open.connections.push((port.id.clone(), connection));
                changed = true;
            }
        }
        Ok(changed)
    }

    fn open_one(
        &self,
        backend: &mut dyn MidiBackend,
        id: &str,
    ) -> Result<Box<dyn MidiInputConnection>, MidiError> {
        let shared = Arc::clone(&self.shared);
        backend.open_input(
            id,
            Box::new(move |bytes: &[u8]| {
                shared.handle(bytes);
            }),
        )
    }
}
