//! Maps [`SampleId`] handles to decoded samples, so `Command::SamplerAssign` carries an id instead
//! of audio. Control thread only.

use super::Sample;
use om_core::sync::LockExt;
use om_core::SampleId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct SampleRegistry {
    next: AtomicU64,
    samples: Mutex<HashMap<u64, Arc<Sample>>>,
}

impl SampleRegistry {
    /// Registers `sample` and returns its handle.
    pub fn register(&self, sample: Arc<Sample>) -> SampleId {
        let id = self.next.fetch_add(1, Relaxed) + 1;
        self.samples.lock_unpoisoned().insert(id, sample);
        SampleId(id)
    }

    pub fn get(&self, id: SampleId) -> Option<Arc<Sample>> {
        self.samples.lock_unpoisoned().get(&id.0).cloned()
    }

    /// Drops a handle. A pad that already holds the sample keeps it.
    pub fn forget(&self, id: SampleId) {
        self.samples.lock_unpoisoned().remove(&id.0);
    }
}
