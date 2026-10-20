//! Scripted DJ for the soak: moves mixer controls and deck rates, and drives loops, hotcues,
//! key-lock, FX and seeks. Control thread only.
//!
//! Gapless actions (loop wraps and exits, key-lock on) are not excused from the underrun budget.
//! Every parameter goes through the router, and a rejected id
//! fails the run rather than being ignored.

use super::tone::Rng;
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::quantizer::Loop;
use om_engine::sampler::PadMode;
use om_engine::{DjEngine, Sample};
use std::sync::Arc;

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
    pub stem_changes: u32,
    /// One-shot and loop starts on the sampler pads.
    pub pad_triggers: u32,
    /// Gate pads pressed and held for a while, then released.
    pub gate_holds: u32,
    /// Samples swapped into a pad while it plays.
    pub pad_replaces: u32,
}

/// Sampler pads in the soak.
const PADS: usize = 8;
/// Pad modes in the soak: a mix of one-shot, gate and loop.
const PAD_MODES: [PadMode; PADS] = [
    PadMode::OneShot,
    PadMode::OneShot,
    PadMode::OneShot,
    PadMode::Gate,
    PadMode::Gate,
    PadMode::Loop,
    PadMode::OneShot,
    PadMode::Loop,
];
/// Seconds into the run when pad 3 gets a new sample while its voices may be sounding.
const REPLACE_AT_SECONDS: f64 = 10.0;
/// Pad that the mid-run replacement targets.
const REPLACED_PAD: usize = 3;

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
    /// Deck 0 plays from stems: toggle its stem mutes, gains and filters.
    stems: bool,
    /// Per pad: when a held gate releases, if one is held.
    gate_until: [Option<u64>; PADS],
    replaced: bool,
}

/// A decaying sine for pad `pad` (pitch rises by a fourth every four pads), half a second long.
fn pad_sample(pad: usize, rate: f64) -> Arc<Sample> {
    let freq = 110.0 * 2f64.powf(pad as f64 / 4.0);
    let frames = (rate * 0.5) as usize;
    let mono: Vec<f32> = (0..frames)
        .map(|i| {
            let t = i as f64 / rate;
            (0.3 * (-t * 6.0).exp() * (std::f64::consts::TAU * freq * t).sin()) as f32
        })
        .collect();
    Arc::new(Sample::from_mono(format!("pad{pad}"), rate, &mono).expect("soak: pad sample"))
}

impl<'a> Scenario<'a> {
    pub fn new(engine: &'a DjEngine, router: &'a ParameterRouter, stems: bool) -> Self {
        Self {
            stems,
            engine,
            router,
            rng: Rng::new(SEED),
            underruns: UnderrunTracker::default(),
            counts: ActionCounts::default(),
            gate_until: [None; PADS],
            replaced: false,
        }
    }

    pub fn begin(&mut self, now_ms: u64) {
        self.underruns.disturb(now_ms);
        self.engine.deck(0).set_keylock(true);
        self.engine.fx().master_rack().set_tempo(120.0);
        self.load_pads();
    }

    /// One control tick. `seconds` is the time since `begin`.
    pub fn step(&mut self, now_ms: u64, seconds: f64) {
        self.wiggle_controls(seconds);
        self.maybe_loop();
        self.maybe_hotcue(now_ms);
        self.maybe_toggle_keylock(now_ms);
        self.maybe_change_fx();
        self.maybe_seek(now_ms);
        self.maybe_change_stems(seconds);
        self.maybe_pads(now_ms, seconds);
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

    /// Fills the eight pads with generated tones and sets their modes. Called once, after the
    /// device has started (the sampler decodes at the engine rate it was prepared with).
    fn load_pads(&self) {
        let rate = self.engine.sampler().engine_rate();
        for (pad, mode) in PAD_MODES.iter().enumerate() {
            self.engine
                .sampler()
                .assign(pad as u8, pad_sample(pad, rate))
                .expect("soak: assign pad sample");
            self.engine.sampler().set_mode(pad as u8, *mode);
        }
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

    /// Sampler: random one-shot and loop triggers, gate holds that release after a while, and a
    /// loop pad released now and then. Once, a new sample replaces pad 3 mid-run.
    fn maybe_pads(&mut self, now_ms: u64, seconds: f64) {
        for (pad, mode) in PAD_MODES.iter().enumerate() {
            if self.gate_until[pad].is_some_and(|until| now_ms >= until) {
                self.gate_until[pad] = None;
                self.set(&format!("sampler.pad{pad}.gate"), 0.0);
            } else if *mode == PadMode::Loop
                && self.gate_until[pad].is_none()
                && self.rng.below(120) == 0
            {
                self.set(&format!("sampler.pad{pad}.trigger"), 0.0);
            }
        }
        if !self.replaced && seconds >= REPLACE_AT_SECONDS {
            self.replaced = true;
            let rate = self.engine.sampler().engine_rate();
            let sample = pad_sample(REPLACED_PAD + 5, rate);
            self.engine
                .sampler()
                .assign(REPLACED_PAD as u8, sample)
                .expect("soak: replace pad sample");
            self.counts.pad_replaces += 1;
        }
        if self.rng.below(8) != 0 {
            return;
        }
        let pad = self.rng.below(PADS);
        if PAD_MODES[pad] == PadMode::Gate && self.gate_until[pad].is_none() {
            let hold_ms = 200 + self.rng.below(1300) as u64;
            self.gate_until[pad] = Some(now_ms + hold_ms);
            self.set(&format!("sampler.pad{pad}.gate"), 1.0);
            self.counts.gate_holds += 1;
        } else if PAD_MODES[pad] != PadMode::Gate {
            let velocity = 0.3 + 0.7 * self.rng.next_unit();
            self.set(&format!("sampler.pad{pad}.trigger"), velocity);
            self.counts.pad_triggers += 1;
        }
    }

    /// Stem mode only: every tick nudges stem volumes and filters; now and then a mute flips.
    fn maybe_change_stems(&mut self, t: f64) {
        if !self.stems {
            return;
        }
        for stem in 0..4 {
            let phase = t * (0.4 + 0.1 * stem as f64) + stem as f64;
            self.set(&format!("stem.0.{stem}.volume"), 0.8 + 0.5 * phase.sin());
            self.set(&format!("stem.0.{stem}.filter"), 0.8 * (phase * 0.6).cos());
        }
        if self.rng.below(6) == 0 {
            let stem = self.rng.below(4);
            let mute = if self.rng.coin() { 1.0 } else { 0.0 };
            self.set(&format!("stem.0.{stem}.mute"), mute);
            self.counts.stem_changes += 1;
        }
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
