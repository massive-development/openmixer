//! Stem control surface: the `stem.<deck>.<stem>.volume|mute|filter` parameter target, the registry
//! that turns a stem directory into a [`StemSetRef`] handle, and a command sink for
//! `Command::LoadStems`. All control-thread; the audio side only sees the deck's
//! [`StemMixerControl`] atomics.

use crate::deck::Deck;
use om_core::param_id::StemParam;
use om_core::sync::LockExt;
use om_core::{Command, CommandError, CommandSink, ParamId, ParamTarget, StemSetRef, StemSnapshot};
use om_dsp::{StemMixerControl, STEM_COUNT};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

/// The `stem` namespace: every deck's stem mixer, routed by the deck index of a `stem.<d>.*` id.
pub(crate) struct StemBank {
    controls: Vec<Arc<StemMixerControl>>,
}

impl StemBank {
    pub(crate) fn new(decks: &[Arc<Deck>]) -> Self {
        Self {
            controls: decks.iter().map(|d| d.stem_control()).collect(),
        }
    }
}

impl ParamTarget for StemBank {
    /// `volume` is the linear gain itself (clamped to 0..1.5), `mute` is on from 0.5, `filter` is
    /// the sweep position (-1..1). Values are stored even while the deck has no stems loaded.
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        let ParamId::Stem { deck, stem, p } = id else {
            return false;
        };
        let stem = usize::from(*stem);
        let Some(control) = self.controls.get(usize::from(*deck)) else {
            return false;
        };
        if stem >= STEM_COUNT || !value.is_finite() {
            return false;
        }
        match p {
            StemParam::Volume => control.set_gain(stem, value as f32),
            StemParam::Mute => control.set_muted(stem, value >= 0.5),
            StemParam::Filter => control.set_filter(stem, value as f32),
        }
        true
    }
}

/// The four stems of a deck as plain values.
pub(crate) fn snapshot_stems(control: &StemMixerControl) -> [StemSnapshot; STEM_COUNT] {
    std::array::from_fn(|stem| StemSnapshot {
        available: control.is_available(),
        muted: control.is_muted(stem),
        gain: control.gain(stem),
        filter: control.filter(stem),
    })
}

/// Maps [`StemSetRef`] handles to stem directories, so commands carry an id instead of a path.
#[derive(Default)]
pub struct StemSetRegistry {
    next: AtomicU64,
    sets: Mutex<HashMap<u64, PathBuf>>,
}

impl StemSetRegistry {
    /// Registers `dir` and returns its handle. Control thread.
    pub fn register(&self, dir: &Path) -> StemSetRef {
        let id = self.next.fetch_add(1, Relaxed) + 1;
        self.sets.lock_unpoisoned().insert(id, dir.to_owned());
        StemSetRef(id)
    }

    /// The directory behind a handle.
    pub fn path(&self, set: StemSetRef) -> Option<PathBuf> {
        self.sets.lock_unpoisoned().get(&set.0).cloned()
    }

    /// Drops a handle.
    pub fn forget(&self, set: StemSetRef) {
        self.sets.lock_unpoisoned().remove(&set.0);
    }
}

/// Handles `Command::LoadStems`; every other command goes to `fallback` when there is one and is
/// rejected otherwise.
pub struct EngineCommands {
    decks: Vec<Arc<Deck>>,
    registry: Arc<StemSetRegistry>,
    fallback: Option<Arc<dyn CommandSink>>,
}

impl EngineCommands {
    pub(crate) fn new(
        decks: Vec<Arc<Deck>>,
        registry: Arc<StemSetRegistry>,
        fallback: Option<Arc<dyn CommandSink>>,
    ) -> Self {
        Self {
            decks,
            registry,
            fallback,
        }
    }
}

impl CommandSink for EngineCommands {
    fn send(&self, cmd: Command) -> Result<(), CommandError> {
        match cmd {
            Command::LoadStems { deck, set } => {
                let target = self
                    .decks
                    .get(usize::from(deck))
                    .ok_or_else(|| CommandError::Rejected(format!("no deck {deck}")))?;
                let dir = self
                    .registry
                    .path(set)
                    .ok_or_else(|| CommandError::Rejected(format!("unknown stem set {}", set.0)))?;
                target
                    .load_stems(&dir)
                    .map_err(|e| CommandError::Rejected(e.to_string()))
            }
            other => match &self.fallback {
                Some(sink) => sink.send(other),
                None => Err(CommandError::Rejected(
                    "only LoadStems is handled by the engine sink".to_owned(),
                )),
            },
        }
    }
}
