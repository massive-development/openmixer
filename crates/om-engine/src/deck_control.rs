//! Deck-level performance controls that need more than the [`Deck`]: sync, quantized loops and slip.
//! Wraps a deck as its `ParameterTarget`: the ids below are handled here, everything else goes to
//! `Deck::set_parameter`.
//!
//! Ids: `sync` (>= 0.5 follows the leader), `sync.master` (>= 0.5 makes this deck the leader, < 0.5
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
use om_core::ParameterTarget;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

const LOOP_QUANTIZED_PREFIX: &str = "loop.quantized.";

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
        self.slip.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn set_leader(&self, on: bool) {
        if on {
            self.sync.set_leader(Some(self.index));
        } else if self.sync.leader() == Some(self.index) {
            // Only the current leader can be released.
            self.sync.set_leader(None);
        }
    }

    /// Returns `None` when `id` is not a sync id.
    fn set_sync_parameter(&self, id: &str, value: f64) -> Option<bool> {
        match id {
            "sync" => {
                self.sync.set_enabled(self.index, value >= 0.5);
                Some(true)
            }
            "sync.master" => {
                self.set_leader(value >= 0.5);
                Some(true)
            }
            _ => None,
        }
    }

    fn set_loop_parameter(&self, text: &str, value: f64) -> bool {
        let Some(beats) = parse_beats(text) else {
            return false;
        };
        // Release is a no-op; press reports whether a loop was set.
        value < 0.5 || self.set_quantized_loop(beats)
    }
}

/// Parses the `<beats>` of `loop.quantized.<beats>`: digits with at most one dot, no leading or
/// trailing dot, and a finite positive value.
fn parse_beats(text: &str) -> Option<f64> {
    let well_formed = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && text.matches('.').count() <= 1
        && !text.starts_with('.')
        && !text.ends_with('.');
    if !well_formed {
        return None;
    }
    let beats: f64 = text.parse().ok()?;
    (beats.is_finite() && beats > 0.0).then_some(beats)
}

impl ParameterTarget for DeckControl {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        if let Some(handled) = self.set_sync_parameter(id, value) {
            return handled;
        }
        if let Some(text) = id.strip_prefix(LOOP_QUANTIZED_PREFIX) {
            return self.set_loop_parameter(text, value);
        }
        match id {
            "slip" => {
                self.set_slip(value >= 0.5);
                true
            }
            "quantize" => {
                self.set_quantize(value >= 0.5);
                true
            }
            _ => self.deck.set_parameter(id, value),
        }
    }
}
