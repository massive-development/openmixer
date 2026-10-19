//! Deck-level performance controls that need more than the [`Deck`]: sync, quantized loops and slip.
//! Handles the `deck.<d>.*` ids it owns and passes the rest to the deck.
//!
//! Params: `sync` (>= 0.5 follows the leader), `sync.master` (>= 0.5 makes this deck the leader, < 0.5
//! releases it), `loop.quantized.<beats>` (>= 0.5 loops `<beats>` from the nearest grid step),
//! `slip` (>= 0.5 on), `quantize` (>= 0.5 snaps quantized loop starts to the grid, the default;
//! < 0.5 starts exactly at the playhead).
//!
//! Threads: control threads only. Several may call it (MIDI, UI, timer); the slip mutex guards
//! slip state and is never taken on the audio thread.

use crate::deck::Deck;
use crate::grid::BeatGrid;
use crate::quantizer::make_quantized_loop;
use crate::slip::SlipTracker;
use crate::sync::Sync;
use om_core::param_id::DeckParam;
use om_core::sync::LockExt;
use om_core::{ParamId, ParamTarget, ParameterTarget};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

pub struct DeckControl {
    deck: Arc<Deck>,
    sync: Arc<Sync>,
    index: usize,
    quantize: AtomicBool,
    slip: Mutex<SlipTracker>,
}

impl DeckControl {
    /// `index` is this deck's position in `sync` (its grid and leader slot).
    pub fn new(deck: Arc<Deck>, sync: Arc<Sync>, index: usize) -> Self {
        Self {
            deck,
            sync,
            index,
            quantize: AtomicBool::new(true),
            slip: Mutex::new(SlipTracker::default()),
        }
    }

    pub fn deck(&self) -> &Deck {
        &self.deck
    }

    /// Plug-in point for the analysis pipeline. `None` removes the grid (no sync or quantize).
    pub fn set_grid(&self, grid: Option<Arc<dyn BeatGrid>>) {
        self.sync.set_grid(self.index, grid);
    }

    pub fn grid(&self) -> Option<Arc<dyn BeatGrid>> {
        self.sync.grid(self.index)
    }

    /// Loops `length_beats` from the nearest grid step to the playhead. False when there is no grid
    /// or the length is not positive.
    pub fn set_quantized_loop(&self, length_beats: f64) -> bool {
        let grid = self.grid();
        // Snap the start to the loop's own length, but never coarser than a beat; 0 = no snapping.
        let division = if self.is_quantize() {
            length_beats.min(1.0)
        } else {
            0.0
        };
        let loop_ = make_quantized_loop(
            grid.as_deref(),
            self.deck.position_frames(),
            length_beats,
            division,
        );
        loop_.valid() && self.deck.set_loop(loop_)
    }

    pub fn set_slip(&self, on: bool) {
        let resume = {
            let mut slip = self.slip_lock();
            if on {
                slip.begin(self.deck.position_frames());
                None
            } else {
                slip.end(self.deck.length_frames())
            }
        };
        if let Some(resume) = resume {
            self.deck.exit_loop();
            self.deck.seek(resume);
        }
    }

    pub fn is_slipping(&self) -> bool {
        self.slip_lock().is_active()
    }

    pub fn set_quantize(&self, on: bool) {
        self.quantize.store(on, Relaxed);
    }

    pub fn is_quantize(&self) -> bool {
        self.quantize.load(Relaxed)
    }

    /// Control-timer step: moves the slip ghost playhead by elapsed time at the current rate.
    pub fn tick(&self, elapsed_seconds: f64) {
        if !self.deck.is_playing() {
            return;
        }
        let frames = elapsed_seconds * self.deck.rate() * self.deck.file_sample_rate();
        self.slip_lock().advance(frames);
    }

    fn slip_lock(&self) -> std::sync::MutexGuard<'_, SlipTracker> {
        self.slip.lock_unpoisoned()
    }

    fn set_leader(&self, on: bool) {
        if on {
            self.sync.set_leader(Some(self.index));
        } else if self.sync.leader() == Some(self.index) {
            // Only the current leader can be released.
            self.sync.set_leader(None);
        }
    }

    /// Applies one deck parameter. Returns `false` when the parameter is not valid here (a quantized
    /// loop with no grid reports failure on press).
    pub fn set_deck_param(&self, p: &DeckParam, value: f64) -> bool {
        match p {
            DeckParam::Sync => {
                self.sync.set_enabled(self.index, value >= 0.5);
                true
            }
            DeckParam::SyncMaster => {
                self.set_leader(value >= 0.5);
                true
            }
            DeckParam::Slip => {
                self.set_slip(value >= 0.5);
                true
            }
            DeckParam::Quantize => {
                self.set_quantize(value >= 0.5);
                true
            }
            // Release is a no-op; press reports whether a loop was set.
            DeckParam::LoopQuantized(beats) => value < 0.5 || self.set_quantized_loop(*beats),
            _ => self.deck.set_deck_param(p, value),
        }
    }
}

impl ParamTarget for DeckControl {
    /// Handles `deck.<d>.<param>` ids; the deck index is the owner's concern.
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        match id {
            ParamId::Deck { p, .. } => self.set_deck_param(p, value),
            _ => false,
        }
    }
}

impl ParameterTarget for DeckControl {
    /// The id after `deck.<d>.`: `sync.master`, `loop.quantized.2`, `hotcue1`, and so on.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match id.parse::<DeckParam>() {
            Ok(p) => self.set_deck_param(&p, value),
            Err(_) => false,
        }
    }
}

/// Every deck of the engine, routed by the deck index of a `deck.<d>.*` id.
pub(crate) struct DeckBank {
    controls: Vec<Arc<DeckControl>>,
}

impl DeckBank {
    pub(crate) fn new(controls: Vec<Arc<DeckControl>>) -> Self {
        Self { controls }
    }
}

impl ParamTarget for DeckBank {
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        match id {
            ParamId::Deck { deck, .. } => self
                .controls
                .get(usize::from(*deck))
                .is_some_and(|control| control.set_param(id, value)),
            _ => false,
        }
    }
}
