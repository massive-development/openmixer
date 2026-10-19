//! Plugin stage factories and the live stages attached to the engine. Control thread only.
//!
//! The engine owns the stage once it is inserted: [`om_engine::StageInsert::set`] takes the box, and
//! the displaced stage is freed on the control thread. The catalog therefore keeps only the
//! bookkeeping (which plugin put a stage where), and clears a slot by inserting `None`.

use crate::api::{PluginError, StageFactory, StageKind};
use om_engine::{DjEngine, MixerControl, ProcessStage};
use std::sync::Arc;

/// Where a stage goes: the master bus or one channel strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertAt {
    Master,
    Channel(usize),
}

/// An insert point refused the stage: the slot is invalid for it or the mixer rejected it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertRefused;

/// Something stages can be inserted into. The engine's mixer control implements it, and so does
/// the engine handle; tests can implement it over a stub.
pub trait InsertPoint: Send + Sync {
    fn channel_count(&self) -> usize;
    /// Inserts (`Some`) or clears (`None`) `slot`. Err when refused.
    fn set_insert(
        &self,
        at: InsertAt,
        slot: usize,
        stage: Option<Box<dyn ProcessStage>>,
    ) -> Result<(), InsertRefused>;
}

impl InsertPoint for MixerControl {
    fn channel_count(&self) -> usize {
        self.num_channels()
    }

    fn set_insert(
        &self,
        at: InsertAt,
        slot: usize,
        stage: Option<Box<dyn ProcessStage>>,
    ) -> Result<(), InsertRefused> {
        let accepted = match at {
            InsertAt::Master => self.set_master_insert(slot, stage),
            // Out of range is refused here: the mixer control clamps channel indices.
            InsertAt::Channel(ch) if ch < self.num_channels() => {
                self.set_channel_insert(ch, slot, stage)
            }
            InsertAt::Channel(_) => false,
        };
        accepted.then_some(()).ok_or(InsertRefused)
    }
}

impl InsertPoint for DjEngine {
    fn channel_count(&self) -> usize {
        self.mixer().channel_count()
    }

    fn set_insert(
        &self,
        at: InsertAt,
        slot: usize,
        stage: Option<Box<dyn ProcessStage>>,
    ) -> Result<(), InsertRefused> {
        self.mixer().set_insert(at, slot, stage)
    }
}

struct Factory {
    id: String,
    owner: String,
    kind: StageKind,
    make: StageFactory,
}

struct Live {
    point: Arc<dyn InsertPoint>,
    at: InsertAt,
    slot: usize,
    owner: String,
}

#[derive(Default)]
pub struct StageCatalog {
    factories: Vec<Factory>,
    live: Vec<Live>,
}

impl StageCatalog {
    /// Adds a factory for `owner`. Refused for an empty or taken id.
    pub fn add(
        &mut self,
        owner: &str,
        stage_id: &str,
        kind: StageKind,
        factory: StageFactory,
    ) -> Result<(), PluginError> {
        if stage_id.is_empty() {
            return Err(PluginError::Rejected("empty stage id".into()));
        }
        if self.contains(stage_id) {
            return Err(PluginError::Rejected(format!(
                "stage {stage_id} already registered"
            )));
        }
        self.factories.push(Factory {
            id: stage_id.to_owned(),
            owner: owner.to_owned(),
            kind,
            make: factory,
        });
        Ok(())
    }

    pub fn contains(&self, stage_id: &str) -> bool {
        self.factories.iter().any(|f| f.id == stage_id)
    }

    /// Stage ids in registration order, optionally of one kind.
    pub fn ids(&self, kind: Option<StageKind>) -> Vec<String> {
        self.factories
            .iter()
            .filter(|f| kind.is_none_or(|k| f.kind == k))
            .map(|f| f.id.clone())
            .collect()
    }

    /// Creates a stage and inserts it at `at`/`slot`. An existing stage there is replaced (the
    /// engine frees the old one).
    pub fn attach(
        &mut self,
        stage_id: &str,
        point: &Arc<dyn InsertPoint>,
        at: InsertAt,
        slot: usize,
    ) -> Result<(), PluginError> {
        let factory = self
            .factories
            .iter()
            .find(|f| f.id == stage_id)
            .ok_or_else(|| PluginError::UnknownStage(stage_id.to_owned()))?;
        if let InsertAt::Channel(ch) = at {
            if ch >= point.channel_count() {
                return Err(PluginError::CannotInsert(stage_id.to_owned()));
            }
        }
        let owner = factory.owner.clone();
        let stage = (factory.make)();
        point
            .set_insert(at, slot, Some(stage))
            .map_err(|InsertRefused| PluginError::CannotInsert(stage_id.to_owned()))?;
        match self.position(point, at, slot) {
            Some(index) => self.live[index].owner = owner,
            None => self.live.push(Live {
                point: Arc::clone(point),
                at,
                slot,
                owner,
            }),
        }
        Ok(())
    }

    /// Clears the slot. A slot with nothing in it is left alone.
    pub fn detach(&mut self, point: &Arc<dyn InsertPoint>, at: InsertAt, slot: usize) {
        if let Some(index) = self.position(point, at, slot) {
            self.remove(index);
        }
    }

    /// Clears every live stage the plugin inserted and drops its factories.
    pub fn remove_owner(&mut self, owner: &str) {
        let mut index = 0;
        while index < self.live.len() {
            if self.live[index].owner == owner {
                self.remove(index);
            } else {
                index += 1;
            }
        }
        self.factories.retain(|f| f.owner != owner);
    }

    fn position(&self, point: &Arc<dyn InsertPoint>, at: InsertAt, slot: usize) -> Option<usize> {
        self.live
            .iter()
            .position(|l| Arc::ptr_eq(&l.point, point) && l.at == at && l.slot == slot)
    }

    fn remove(&mut self, index: usize) {
        let live = self.live.remove(index);
        // Clearing a slot the point already dropped is harmless; nothing to report.
        let _ = live.point.set_insert(live.at, live.slot, None);
    }
}
