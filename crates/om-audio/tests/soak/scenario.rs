//! Scripted DJ for the soak: moves mixer controls and deck rates, and drives loops, hotcues,
//! key-lock, FX and seeks. Control thread only. Port of `tools/soak/Scenario.h`.
//!
//! Gapless actions (loop wraps and exits, key-lock on) are not excused from the underrun budget.
//! Every parameter goes through the router, as the C++ `setParameter` calls do, and a rejected id
//! fails the run rather than being ignored.

use super::tone::Rng;
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::quantizer::Loop;
use om_engine::DjEngine;

const CONTROL_PERIOD_MS: u64 = 50;
const SAMPLE_RATE: f64 = 48_000.0;
/// 120 BPM at the soak's 48 kHz.
const FRAMES_PER_BEAT: f64 = SAMPLE_RATE * 60.0 / 120.0;
const SEED: u64 = 1234;
/// Underruns right after a seek or restart are expected while the decoder refills; only
/// steady-state ones count.
const SETTLE_MS: u64 = 500;

pub const CONTROL_PERIOD: std::time::Duration = std::time::Duration::from_millis(CONTROL_PERIOD_MS);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActionCounts {
    pub loops: u32,
    pub loop_exits: u32,
    pub hotcue_jumps: u32,
    pub keylock_toggles: u32,
    pub fx_changes: u32,
    pub seeks: u32,
}

#[derive(Default)]
struct UnderrunTracker {
    last: u32,
    steady: u32,
    quiet_until_ms: u64,
}

impl UnderrunTracker {
    fn disturb(&mut self, now_ms: u64) {
        self.quiet_until_ms = now_ms + SETTLE_MS;
    }

    fn tick(&mut self, engine: &DjEngine, now_ms: u64) {
        let total: u32 = (0..engine.num_decks())
            .map(|d| engine.deck(d).underrun_count())
            .sum();
        if now_ms >= self.quiet_until_ms {
            self.steady += total.saturating_sub(self.last);
        }
        self.last = total;
    }
}

pub struct Scenario<'a> {
    engine: &'a DjEngine,
    router: &'a ParameterRouter,
    rng: Rng,
    underruns: UnderrunTracker,
    counts: ActionCounts,
}

impl<'a> Scenario<'a> {
    pub fn new(engine: &'a DjEngine, router: &'a ParameterRouter) -> Self {
        Self {
            engine,
            router,
            rng: Rng::new(SEED),
            underruns: UnderrunTracker::default(),
            counts: ActionCounts::default(),
        }
    }

    pub fn begin(&mut self, now_ms: u64) {
        self.underruns.disturb(now_ms);
        self.engine.deck(0).set_keylock(true);
        self.engine.fx().master_rack().set_tempo(120.0);
    }

    /// One control tick. `seconds` is the time since `begin`.
    pub fn step(&mut self, now_ms: u64, seconds: f64) {
        self.wiggle_controls(seconds);
        self.maybe_loop();
        self.maybe_hotcue(now_ms);
        self.maybe_toggle_keylock(now_ms);
        self.maybe_change_fx();
        self.maybe_seek(now_ms);
        self.restart_finished_decks(now_ms);
        self.underruns.tick(self.engine, now_ms);
    }

    /// Steady-state underruns across all decks (after the settle windows).
    pub fn steady_underruns(&self) -> u32 {
        self.underruns.steady
    }

    pub fn counts(&self) -> ActionCounts {
        self.counts
    }

    fn set(&self, id: &str, value: f64) {
        assert!(
            self.router.set_parameter(id, value),
            "soak: unknown parameter {id}"
        );
    }

    fn random_deck(&mut self) -> usize {
        self.rng.below(self.engine.num_decks())
    }

    fn wiggle_controls(&mut self, t: f64) {
        self.set("mixer.crossfader", 0.5 + 0.5 * (t * 0.7).sin());
        for ch in 0..2 {
            let prefix = format!("mixer.ch{ch}");
            let low = -12.0 + 18.0 * self.rng.next_unit();
            let high = -12.0 + 18.0 * self.rng.next_unit();
            self.set(&format!("{prefix}.eq.low"), low);
            self.set(&format!("{prefix}.eq.high"), high);
            self.set(
                &format!("{prefix}.filter"),
                0.5 + 0.5 * (t * (0.3 + 0.2 * ch as f64)).sin(),
            );
            let fader = 0.6 + 0.4 * self.rng.next_unit();
            self.set(&format!("{prefix}.fader"), fader);
            let rate = 0.94 + 0.12 * self.rng.next_unit();
            self.set(&format!("deck.{ch}.rate"), rate);
        }
    }

    /// Loops from 1/4 to 4 beats at the playhead; held a few seconds, then exited.
    fn maybe_loop(&mut self) {
        if self.rng.below(60) != 0 {
            return;
        }
        let deck = self.engine.deck(self.random_deck());
        if deck.is_looping() {
            deck.exit_loop();
            self.counts.loop_exits += 1;
            return;
        }
        // 100 ms ahead leaves time to prefetch.
        let start = deck.position_frames() + 4800;
        let beats = 0.25 * f64::from(1_u32 << self.rng.below(5));
        let end = start + (beats * FRAMES_PER_BEAT) as i64;
        if deck.set_loop(Loop { start, end }) {
            self.counts.loops += 1;
        }
    }

    fn maybe_hotcue(&mut self, now_ms: u64) {
        if self.rng.below(80) != 0 {
            return;
        }
        let deck = self.engine.deck(self.random_deck());
        let cue = self.rng.below(4);
        if deck.hotcue(cue).is_some() {
            self.counts.hotcue_jumps += 1;
            self.underruns.disturb(now_ms); // a jump is a seek: the ring refills
        }
        deck.trigger_hotcue(cue);
    }

    /// Deck 1 flips key-lock; switching off drops stretcher input, which can starve briefly.
    fn maybe_toggle_keylock(&mut self, now_ms: u64) {
        if self.rng.below(200) != 0 {
            return;
        }
        let deck = self.engine.deck(1);
        deck.set_keylock(!deck.is_keylock());
        self.counts.keylock_toggles += 1;
        self.underruns.disturb(now_ms);
    }

    /// Flips one FX unit on master or a channel rack and sets a random wet mix. Tails ring out on
    /// disable.
    fn maybe_change_fx(&mut self) {
        let units = self.engine.fx().master_rack().num_units();
        if self.rng.below(30) != 0 || units == 0 {
            return;
        }
        let pick = self.rng.below(self.engine.num_decks() + 1); // last = master
        let rack = if pick == self.engine.num_decks() {
            "master".to_owned()
        } else {
            format!("ch{pick}")
        };
        let unit = format!("fx.{rack}.fx{}", self.rng.below(units));
        let enable = if self.rng.coin() { 1.0 } else { 0.0 };
        self.set(&format!("{unit}.enable"), enable);
        let mix = 0.2 + 0.6 * self.rng.next_unit();
        self.set(&format!("{unit}.mix"), mix);
        self.counts.fx_changes += 1;
    }

    fn maybe_seek(&mut self, now_ms: u64) {
        if self.rng.below(40) != 0 {
            return;
        }
        let deck = self.random_deck();
        let position = self.rng.next_unit();
        self.set(&format!("deck.{deck}.seek"), position);
        self.counts.seeks += 1;
        self.underruns.disturb(now_ms);
    }

    fn restart_finished_decks(&mut self, now_ms: u64) {
        for d in 0..self.engine.num_decks() {
            let deck = self.engine.deck(d);
            if deck.has_finished() {
                deck.seek(0);
                deck.play();
                self.underruns.disturb(now_ms);
            }
        }
    }
}
