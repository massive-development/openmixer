//! Routes live MIDI bytes into a [`MidiMapper`], with optional recording. Device enumeration and
//! connection (midir) are not in this crate yet: midir cannot join the workspace while the cpal and
//! gpui dependency pins stand. Input bytes reach this type through [`MidiInputRouter::handle_incoming`].

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

/// Routes live MIDI bytes into a [`MidiMapper`] and records them while recording. Device
/// enumeration and connection (midir) are not in this crate yet; see the module note.
pub struct MidiInputRouter {
    shared: Arc<Shared>,
}

impl MidiInputRouter {
    pub fn new(mapper: Arc<MidiMapper>) -> Self {
        Self {
            shared: Arc::new(Shared {
                mapper,
                recording: Mutex::new(None),
            }),
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
}
