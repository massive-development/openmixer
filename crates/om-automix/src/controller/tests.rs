use super::*;
use crate::fixtures::{info, LENGTH_BEATS};
use crate::readback::blank_snapshot;
use crate::stem_fade::CancelMode;
use om_core::param_id::{AutomixParam, EqBand, StemParam};
use om_core::{AutomixStyle, ParamTarget};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const DT: f64 = 1.0 / 30.0;
const RATE: f64 = 44_100.0;
const BPM: f64 = 120.0;
const FRAMES_PER_BEAT: f64 = RATE * 60.0 / BPM;

#[derive(Default)]
struct Log {
    cmds: Mutex<Vec<Command>>,
    closed: AtomicBool,
}

impl CommandSink for Log {
    fn send(&self, cmd: Command) -> Result<(), CommandError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(CommandError::Closed);
        }
        self.cmds.lock().unwrap().push(cmd);
        Ok(())
    }
}

#[derive(Default)]
struct FakeHost {
    queue: Vec<NextTrack>,
    tracks: HashMap<i64, DeckInfo>,
    infos: HashMap<u8, DeckInfo>,
    starts: Vec<(u8, i64)>,
    pending: Option<(u8, u32)>,
    ready: Vec<u8>,
    fail: Option<String>,
    never_ready: bool,
    underruns: u64,
}

impl AutomixHost for FakeHost {
    fn next_track(&mut self, _playing: u8) -> Option<NextTrack> {
        (!self.queue.is_empty()).then(|| self.queue.remove(0))
    }

    fn start_load(&mut self, deck: u8, track: &NextTrack) -> Result<(), String> {
        self.starts.push((deck, track.id));
        let mut info = self.tracks.get(&track.id).cloned().ok_or("unknown track")?;
        info.deck = deck;
        self.infos.insert(deck, info);
        self.pending = Some((deck, 3));
        Ok(())
    }

    fn poll_load(&mut self, deck: u8) -> LoadPoll {
        if let Some(error) = &self.fail {
            return LoadPoll::Failed(error.clone());
        }
        if self.ready.contains(&deck) {
            LoadPoll::Ready
        } else {
            LoadPoll::Pending
        }
    }

    fn deck_info(&mut self, deck: u8) -> Option<DeckInfo> {
        self.infos.get(&deck).cloned()
    }

    fn deck_title(&mut self, _deck: u8) -> String {
        "cued by hand".to_owned()
    }

    fn underruns(&self) -> u64 {
        self.underruns
    }
}

struct Sim {
    ctrl: AutomixController,
    control: Arc<AutomixControl>,
    snap: EngineSnapshot,
    sink: Log,
    host: FakeHost,
    now: f64,
    n: u32,
    dt: f64,
    #[allow(dead_code)]
    fx: Vec<(String, f64)>,
    states: Vec<AutomixState>,
}

fn beat(b: f64) -> i64 {
    (b * FRAMES_PER_BEAT) as i64
}

impl Sim {
    /// Deck 0 plays at `at_beat`, deck 1 is empty, one track queued.
    fn new(at_beat: f64, stems: bool) -> Self {
        Self::with(
            AutomixConfig::default(),
            at_beat,
            stems,
            info(1, BPM, "8A", stems),
        )
    }

    fn with(cfg: AutomixConfig, at_beat: f64, stems: bool, next: DeckInfo) -> Self {
        let control = Arc::new(AutomixControl::new());
        control.set_enabled(true);
        control.set_style(AutomixStyle::Crossfade);
        let mut snap = blank_snapshot(2);
        for d in &mut snap.decks {
            d.length_frames = beat(LENGTH_BEATS);
        }
        snap.decks[0].playing = true;
        snap.decks[0].position_frames = beat(at_beat);
        snap.decks[1].loaded = false;
        snap.decks[1].length_frames = 0;
        if stems {
            snap.stems[0] = [om_core::StemSnapshot {
                available: true,
                ..Default::default()
            }; 4];
        }
        let mut host = FakeHost::default();
        host.infos.insert(0, info(0, BPM, "8A", stems));
        host.tracks.insert(7, next);
        host.queue.push(NextTrack {
            id: 7,
            title: "Next One".into(),
            bpm: None,
            key: String::new(),
        });
        Self {
            ctrl: AutomixController::new(cfg, Arc::clone(&control)),
            control,
            snap,
            sink: Log::default(),
            host,
            now: 0.0,
            n: 0,
            dt: DT,
            fx: Vec::new(),
            states: Vec::new(),
        }
    }

    fn cmds(&self) -> Vec<Command> {
        self.sink.cmds.lock().unwrap().clone()
    }

    fn sent(&self, id: &str) -> Vec<f64> {
        self.cmds()
            .iter()
            .filter_map(|c| match c {
                Command::Param { id: i, value } if i.to_string() == id => Some(*value),
                _ => None,
            })
            .collect()
    }

    /// The world moves one tick, then the controller runs and its commands take effect.
    fn tick(&mut self) {
        self.now += self.dt;
        self.n += 1;
        for d in &mut self.snap.decks {
            if d.playing && d.loaded {
                d.position_frames += (self.dt * RATE) as i64;
            }
        }
        if let Some((deck, after)) = self.host.pending {
            if self.host.fail.is_some() {
                self.host.pending = None;
            } else if self.n >= after {
                let d = &mut self.snap.decks[usize::from(deck)];
                d.loaded = true;
                d.position_frames = 0;
                d.length_frames = beat(LENGTH_BEATS);
                if let Some(i) = self.host.infos.get(&deck) {
                    if i.has_stems {
                        self.snap.stems[usize::from(deck)] = [om_core::StemSnapshot {
                            available: true,
                            ..Default::default()
                        }; 4];
                    }
                }
                if !self.host.never_ready {
                    self.host.ready.push(deck);
                }
                self.host.pending = None;
            }
        }
        self.snap.ticks += 1;
        let before = self.sink.cmds.lock().unwrap().len();
        self.ctrl
            .tick(self.now, &self.snap, &self.sink, &mut self.host);
        let new: Vec<Command> = self.sink.cmds.lock().unwrap()[before..].to_vec();
        for c in &new {
            self.apply(c);
        }
        let state = self.ctrl.state();
        if self.states.last() != Some(&state) {
            self.states.push(state);
        }
    }

    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    fn run_until(&mut self, what: &str, max: u32, done: impl Fn(&Sim) -> bool) {
        for _ in 0..max {
            if done(self) {
                return;
            }
            self.tick();
        }
        panic!(
            "never reached: {what} (state {:?}, error {:?})",
            self.ctrl.state(),
            self.ctrl.snapshot().last_error
        );
    }

    fn apply(&mut self, cmd: &Command) {
        let Command::Param { id, value } = cmd else {
            return;
        };
        let v = *value;
        match id {
            ParamId::Mixer(MixerParam::Crossfader) => self.snap.mixer.crossfader = v as f32,
            ParamId::Mixer(MixerParam::Channel { ch, p }) => {
                let c = &mut self.snap.mixer.channels[usize::from(*ch)];
                match p {
                    ChannelParam::Fader => c.fader = v as f32,
                    ChannelParam::Filter => c.filter = v as f32,
                    ChannelParam::Eq(b) => {
                        c.eq[match b {
                            EqBand::Low => 0,
                            EqBand::Mid => 1,
                            EqBand::High => 2,
                        }] = v as f32
                    }
                    _ => {}
                }
            }
            ParamId::Deck { deck, p } => {
                let len = self.snap.decks[usize::from(*deck)].length_frames;
                match p {
                    DeckParam::Play => self.snap.decks[usize::from(*deck)].playing = v >= 0.5,
                    DeckParam::Sync => self.snap.decks[usize::from(*deck)].sync = v >= 0.5,
                    DeckParam::SyncMaster => {
                        for d in &mut self.snap.decks {
                            d.sync_master = false;
                        }
                        self.snap.decks[usize::from(*deck)].sync_master = v >= 0.5;
                    }
                    DeckParam::Seek => {
                        self.snap.decks[usize::from(*deck)].position_frames =
                            (v * len as f64) as i64
                    }
                    _ => {}
                }
            }
            ParamId::Stem { deck, stem, p } => {
                let s = &mut self.snap.stems[usize::from(*deck)][usize::from(*stem)];
                match p {
                    StemParam::Volume => s.gain = v as f32,
                    StemParam::Mute => s.muted = v >= 0.5,
                    StemParam::Filter => s.filter = v as f32,
                }
            }
            other => self.fx.push((other.to_string(), v)),
        }
    }

    fn state(&self) -> AutomixState {
        self.ctrl.state()
    }

    fn xfader(&self) -> f32 {
        self.snap.mixer.crossfader
    }

    /// Runs until the transition is some way in.
    fn run_into_transition(&mut self) {
        self.run_until("transitioning", 3000, |s| {
            s.state() == AutomixState::Transitioning
        });
        self.run(60);
        assert_eq!(self.state(), AutomixState::Transitioning);
        assert!(
            self.xfader() > 0.05 && self.xfader() < 0.9,
            "{}",
            self.xfader()
        );
    }
}

fn eq_low(ch: u8) -> ParamId {
    ParamId::Mixer(MixerParam::Channel {
        ch,
        p: ChannelParam::Eq(EqBand::Low),
    })
}

#[test]
fn happy_path_lands_the_crossfader_and_hands_over_the_decks() {
    let mut sim = Sim::new(380.0, false);
    sim.run_until("idle after the transition", 4000, |s| {
        s.states.contains(&AutomixState::Cleanup) && s.state() == AutomixState::Idle
    });
    assert_eq!(
        sim.states,
        vec![
            AutomixState::Idle,
            AutomixState::Loading,
            AutomixState::Armed,
            AutomixState::Transitioning,
            AutomixState::Cleanup,
            AutomixState::Idle
        ]
    );
    assert_eq!(sim.xfader(), 1.0, "the crossfader lands exactly");
    assert!(!sim.snap.decks[0].playing, "outgoing deck stopped");
    assert!(sim.snap.decks[1].playing, "incoming deck plays");
    assert!(sim.snap.decks[1].sync, "in sync with the leader");
    assert!(sim.snap.decks[1].sync_master && !sim.snap.decks[0].sync_master);
    assert!(!sim.snap.decks[0].sync);
    // The bass EQs are neutral again on both channels.
    assert_eq!(sim.snap.mixer.channels[0].eq[0], 0.0);
    assert_eq!(sim.snap.mixer.channels[1].eq[0], 0.0);
    // The incoming deck started on the bar, at the plan's start beat, on the outgoing grid.
    let play_at = sim.sent("deck.1.play");
    assert_eq!(play_at, vec![1.0]);
    assert_eq!(sim.host.starts, vec![(1, 7)]);
    assert!(sim.ctrl.snapshot().last_error.is_empty());
}

#[test]
fn the_incoming_deck_is_cued_at_its_mix_in_point_before_the_transition() {
    let mut next = info(1, BPM, "8A", false);
    next.mix_in = Some(32.0);
    let mut sim = Sim::with(AutomixConfig::default(), 380.0, false, next);
    sim.run_until("armed", 600, |s| s.state() == AutomixState::Armed);
    let seeks = sim.sent("deck.1.seek");
    assert_eq!(seeks.len(), 1);
    assert!((seeks[0] - 32.0 / LENGTH_BEATS).abs() < 1e-6, "{seeks:?}");
    assert!((sim.snap.decks[1].position_frames - beat(32.0)).abs() < 2000);
    // Nothing else is sent while armed, and the deck stays cued.
    let sent = sim.cmds().len();
    sim.run(50);
    assert_eq!(sim.cmds().len(), sent);
    assert_eq!(sim.state(), AutomixState::Armed);
}

#[test]
fn the_snapshot_reports_progress_and_the_next_track() {
    let mut sim = Sim::new(380.0, false);
    sim.run_until("loading", 600, |s| s.state() == AutomixState::Loading);
    let s = sim.ctrl.snapshot();
    assert_eq!(s.next_track_title, "Next One");
    assert!(s.enabled);
    assert_eq!(s.style, AutomixStyle::Crossfade);
    assert!(s.remaining_beats.unwrap() <= 96.0);
    sim.run_until("armed", 100, |s| s.state() == AutomixState::Armed);
    assert_eq!(sim.ctrl.snapshot().plan_style, Some(Style::Crossfade));
    sim.run_into_transition();
    let s = sim.ctrl.snapshot();
    assert!(s.progress > 0.0 && s.progress < 1.0);
    assert_eq!(s.state, AutomixState::Transitioning);
}

#[test]
fn nothing_is_sent_while_idle_or_far_from_the_end() {
    let mut sim = Sim::new(100.0, false);
    sim.run(300);
    assert_eq!(sim.state(), AutomixState::Idle);
    assert!(sim.cmds().is_empty());
    assert!(sim.host.starts.is_empty());
    // Disabled: even at the end of the track nothing happens.
    let mut sim = Sim::new(390.0, false);
    sim.control.set_enabled(false);
    sim.run(600);
    assert!(sim.cmds().is_empty() && sim.host.starts.is_empty());
    assert_eq!(sim.state(), AutomixState::Idle);
    assert!(!sim.ctrl.snapshot().enabled);
    // Two playing decks mean the user is mixing: automix stays out.
    let mut sim = Sim::new(390.0, false);
    sim.snap.decks[1].playing = true;
    sim.run(300);
    assert!(sim.cmds().is_empty() && sim.host.starts.is_empty());
}

#[test]
fn disabling_mid_transition_cancels_and_stays_quiet() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    sim.control.set_enabled(false);
    sim.tick();
    assert_eq!(sim.state(), AutomixState::Idle);
    let at = sim.cmds().len();
    sim.run(100);
    assert_eq!(sim.cmds().len(), at, "no commands after the cancel");
    // The crossfader stays where it was.
    assert!(sim.xfader() > 0.05 && sim.xfader() < 0.9);
}

fn assert_cancelled(sim: &mut Sim, expect: &str) -> usize {
    sim.tick();
    assert_eq!(sim.state(), AutomixState::Idle, "{:?}", sim.ctrl.snapshot());
    let error = sim.ctrl.snapshot().last_error;
    assert!(
        error.contains(expect),
        "{error:?} should mention {expect:?}"
    );
    let at = sim.cmds().len();
    sim.run(120);
    assert_eq!(sim.cmds().len(), at, "nothing is sent after a cancel");
    assert_eq!(sim.state(), AutomixState::Idle, "the track is not retried");
    at
}

#[test]
fn a_manual_crossfader_move_cancels_without_snapping_back() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    sim.snap.mixer.crossfader = 0.9;
    let before = sim.cmds().len();
    assert_cancelled(&mut sim, "mixer.crossfader");
    let after = sim.cmds()[before..].to_vec();
    assert!(
        after.iter().all(|c| !matches!(c, Command::Param { id, .. } if *id == ParamId::Mixer(MixerParam::Crossfader))),
        "{after:?}"
    );
    assert_eq!(sim.xfader(), 0.9, "the user's value stays");
    // The bass EQ kill is undone (the user did not touch it).
    assert_eq!(sim.snap.mixer.channels[1].eq[0], 0.0);
}

#[test]
fn every_manual_input_on_the_pair_cancels() {
    type Poke = fn(&mut Sim);
    let cases: Vec<(&str, Poke, &str)> = vec![
        (
            "fader out",
            |s| s.snap.mixer.channels[0].fader = 0.4,
            "mixer.ch0.fader",
        ),
        (
            "fader in",
            |s| s.snap.mixer.channels[1].fader = 0.4,
            "mixer.ch1.fader",
        ),
        ("pause out", |s| s.snap.decks[0].playing = false, "deck 0"),
        (
            "pause in",
            |s| s.snap.decks[1].playing = false,
            "deck.1.play",
        ),
        (
            "seek out forward",
            |s| s.snap.decks[0].position_frames += beat(20.0),
            "deck 0 seeked",
        ),
        (
            "seek out back",
            |s| s.snap.decks[0].position_frames -= beat(20.0),
            "deck 0 seeked",
        ),
        (
            "seek in",
            |s| s.snap.decks[1].position_frames += beat(30.0),
            "deck 1 seeked",
        ),
        (
            "load over out",
            |s| s.snap.decks[0].length_frames += 5000,
            "deck 0",
        ),
        (
            "load over in",
            |s| s.snap.decks[1].length_frames += 5000,
            "deck 1",
        ),
        ("unload in", |s| s.snap.decks[1].loaded = false, "deck 1"),
        (
            "eq in",
            |s| s.snap.mixer.channels[1].eq[0] = -20.0,
            "mixer.ch1.eq.low",
        ),
        ("sync off", |s| s.snap.decks[1].sync = false, "deck.1.sync"),
    ];
    for (name, poke, expect) in cases {
        let mut sim = Sim::new(380.0, false);
        sim.run_into_transition();
        poke(&mut sim);
        assert_cancelled(&mut sim, expect);
        eprintln!("ok: {name}");
    }
}

#[test]
fn input_reported_by_the_binding_layer_cancels() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    sim.ctrl
        .notify_user_input(&ParamId::Mixer(MixerParam::Crossfader));
    assert_cancelled(&mut sim, "mixer.crossfader");

    // Input on an uninvolved channel or on automix itself is ignored.
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    sim.ctrl
        .notify_user_input(&ParamId::Mixer(MixerParam::Channel {
            ch: 3,
            p: ChannelParam::Fader,
        }));
    sim.ctrl
        .notify_user_input(&ParamId::Automix(AutomixParam::Style));
    sim.run(5);
    assert_eq!(sim.state(), AutomixState::Transitioning);

    // A deck button on the involved decks cancels, even before the transition begins.
    let mut sim = Sim::new(380.0, false);
    sim.run_until("armed", 600, |s| s.state() == AutomixState::Armed);
    sim.ctrl.notify_user_input(&ParamId::Deck {
        deck: 1,
        p: DeckParam::Play,
    });
    sim.tick();
    assert_eq!(sim.state(), AutomixState::Idle);
}

#[test]
fn automix_cancel_stops_the_plan_and_skips_the_track() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    assert!(sim
        .control
        .set_param(&ParamId::Automix(AutomixParam::Cancel), 1.0));
    assert_cancelled(&mut sim, "cancelled");
    assert_eq!(sim.host.starts.len(), 1);
}

#[test]
fn an_underrun_cancels_and_turns_automix_off() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    sim.host.underruns += 1;
    assert_cancelled(&mut sim, "underrun");
    assert!(!sim.control.is_enabled());
    assert!(!sim.ctrl.snapshot().enabled);
}

#[test]
fn a_closed_sink_cancels() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    sim.sink.closed.store(true, Ordering::Relaxed);
    sim.tick();
    assert_eq!(sim.state(), AutomixState::Idle);
    assert!(sim.ctrl.snapshot().last_error.contains("engine closed"));
}

#[test]
fn cancel_leave_mode_keeps_stem_moves_and_restore_off_keeps_eq() {
    let cfg = AutomixConfig {
        restore_on_cancel: false,
        stem_cancel: CancelMode::Leave,
        ..AutomixConfig::default()
    };
    let mut sim = Sim::with(cfg, 380.0, false, info(1, BPM, "8A", false));
    sim.run_into_transition();
    sim.snap.mixer.crossfader = 0.9;
    sim.tick();
    assert_eq!(
        sim.snap.mixer.channels[1].eq[0], -48.0,
        "left as the plan had it"
    );
}

#[test]
fn a_cancel_before_the_midpoint_restores_the_killed_bass() {
    let mut sim = Sim::new(380.0, false);
    sim.run_into_transition();
    assert_eq!(
        sim.snap.mixer.channels[1].eq[0], -48.0,
        "incoming bass is cut at first"
    );
    sim.control.set_enabled(false);
    sim.tick();
    assert_eq!(sim.snap.mixer.channels[1].eq[0], 0.0);
}

#[test]
fn a_failed_load_is_reported_and_retried_then_automix_gives_up() {
    let mut sim = Sim::new(380.0, false);
    sim.host.fail = Some("bad file".into());
    for id in 8..12 {
        sim.host.tracks.insert(id, info(1, BPM, "8A", false));
        sim.host.queue.push(NextTrack {
            id,
            title: format!("t{id}"),
            bpm: None,
            key: String::new(),
        });
    }
    sim.run_until("first failure", 600, |s| {
        s.ctrl.snapshot().last_error.contains("could not load")
    });
    assert!(sim.ctrl.snapshot().last_error.contains("bad file"));
    assert_eq!(sim.state(), AutomixState::Idle);
    sim.run(300);
    assert!(sim.host.starts.len() >= 3, "{:?}", sim.host.starts);
    assert!(!sim.control.is_enabled(), "off after repeated failures");
    assert!(sim.ctrl.snapshot().last_error.contains("off"));
    assert!(sim.cmds().is_empty(), "a failed load sends no commands");
}

#[test]
fn a_load_that_never_finishes_times_out() {
    let mut sim = Sim::new(380.0, false);
    sim.host.never_ready = true;
    sim.run_until("loading", 600, |s| s.state() == AutomixState::Loading);
    sim.run_until("timeout", 1200, |s| s.state() == AutomixState::Idle);
    assert!(sim.ctrl.snapshot().last_error.contains("timed out"));
}

#[test]
fn an_empty_queue_is_reported_once_and_not_retried() {
    let mut sim = Sim::new(380.0, false);
    sim.host.queue.clear();
    sim.run(200);
    assert_eq!(sim.state(), AutomixState::Idle);
    assert!(sim.ctrl.snapshot().last_error.contains("queue"));
    assert!(sim.host.starts.is_empty());
}

#[test]
fn an_unplannable_pair_is_dropped_and_not_retried() {
    // 140 vs 120 BPM with an explicit Crossfade: out of range.
    let mut sim = Sim::with(
        AutomixConfig::default(),
        380.0,
        false,
        info(1, 140.0, "8A", false),
    );
    sim.run(600);
    assert_eq!(sim.state(), AutomixState::Idle);
    let error = sim.ctrl.snapshot().last_error;
    assert!(error.starts_with("no transition"), "{error}");
    assert_eq!(sim.host.starts.len(), 1, "the track had its turn");
    assert!(sim.sent("deck.1.play").is_empty());
}

#[test]
fn auto_style_cuts_when_the_tempo_gap_is_too_wide() {
    let mut sim = Sim::with(
        AutomixConfig::default(),
        380.0,
        false,
        info(1, 140.0, "8A", false),
    );
    sim.control.set_style(AutomixStyle::Auto);
    sim.run_until("done", 3000, |s| s.states.contains(&AutomixState::Cleanup));
    assert_eq!(sim.xfader(), 1.0);
    assert!(sim.snap.decks[1].playing);
}

#[test]
fn stems_on_both_decks_swap_the_bass_with_stems() {
    let mut sim = Sim::with(
        AutomixConfig::default(),
        380.0,
        true,
        info(1, BPM, "9A", true),
    );
    sim.control.set_style(AutomixStyle::Auto);
    sim.run_until("transition", 3000, |s| {
        s.state() == AutomixState::Transitioning
    });
    assert_eq!(sim.ctrl.snapshot().plan_style, Some(Style::BassSwap));
    sim.run_until("cleanup", 3000, |s| {
        s.states.contains(&AutomixState::Cleanup)
    });
    assert_eq!(sim.snap.stems[1][1].gain, 1.0, "incoming bass fully in");
    assert!(!sim.sent("stem.1.1.volume").is_empty());
    assert_eq!(
        sim.sent("stem.0.1.volume").last(),
        Some(&1.0),
        "outgoing bass put back after the stop"
    );
    assert!(
        sim.sent("stem.0.1.volume").contains(&0.0),
        "and swapped out before"
    );
    // No EQ moves with stems.
    assert!(sim.sent(&eq_low(0).to_string()).is_empty());
    assert_eq!(sim.xfader(), 1.0);
}

#[test]
fn without_stems_the_swap_falls_back_to_the_eqs() {
    let mut sim = Sim::new(380.0, false);
    sim.control.set_style(AutomixStyle::BassSwap);
    sim.run_until("cleanup", 3000, |s| {
        s.states.contains(&AutomixState::Cleanup)
    });
    assert!(sim.sent("stem.0.1.volume").is_empty() && sim.sent("stem.1.1.volume").is_empty());
    assert!(sim.sent(&eq_low(1).to_string()).contains(&-48.0));
}

#[test]
fn a_stalled_control_clock_still_converges() {
    // The UI froze: ticks arrive 1.5 s apart (3 beats), so ramps are crossed in big jumps.
    let mut sim = Sim::new(380.0, false);
    sim.dt = 1.5;
    sim.run_until("idle after the transition", 400, |s| {
        s.states.contains(&AutomixState::Cleanup) && s.state() == AutomixState::Idle
    });
    assert_eq!(sim.xfader(), 1.0);
    assert!(!sim.snap.decks[0].playing && sim.snap.decks[1].playing);
    assert_eq!(sim.snap.mixer.channels[0].eq[0], 0.0);
    assert_eq!(sim.snap.mixer.channels[1].eq[0], 0.0);
}

#[test]
fn the_load_may_finish_after_the_planned_start() {
    // Only 20 beats left when the controller first sees the deck, and the load takes a while.
    let mut sim = Sim::new(462.0, false);
    sim.host.tracks.get_mut(&7).unwrap().length_beats = LENGTH_BEATS;
    sim.run_until("transition or error", 2000, |s| {
        s.state() == AutomixState::Transitioning
            || s.state() == AutomixState::Idle && !s.cmds().is_empty()
    });
    // Either it found room for a short mix, or it said why not; it never overran the track.
    if sim.state() == AutomixState::Transitioning {
        assert!(sim.ctrl.plan().unwrap().end_beat() <= LENGTH_BEATS);
    }
}

#[test]
fn a_track_cued_by_hand_on_the_free_deck_is_used_as_it_is() {
    let mut sim = Sim::new(380.0, false);
    sim.snap.decks[1].loaded = true;
    sim.snap.decks[1].length_frames = beat(LENGTH_BEATS);
    sim.snap.decks[1].position_frames = beat(64.0);
    sim.host.infos.insert(1, info(1, BPM, "8A", false));
    sim.run_until("armed", 600, |s| s.state() == AutomixState::Armed);
    assert!(sim.host.starts.is_empty(), "no load over the user's track");
    assert!(
        sim.sent("deck.1.seek").is_empty(),
        "their cue point is kept"
    );
    assert_eq!(sim.ctrl.plan().unwrap().in_cue_beat, 64.0);
    sim.run_until("done", 3000, |s| s.states.contains(&AutomixState::Cleanup));
    assert_eq!(sim.xfader(), 1.0);
}

#[test]
fn the_user_playing_the_cued_deck_disarms() {
    let mut sim = Sim::new(380.0, false);
    sim.run_until("armed", 600, |s| s.state() == AutomixState::Armed);
    sim.snap.decks[1].playing = true;
    sim.tick();
    assert_eq!(sim.state(), AutomixState::Idle);
    sim.run(100);
    assert!(sim.sent("mixer.crossfader").is_empty());
}

#[test]
fn seeking_the_cued_deck_disarms() {
    let mut sim = Sim::new(380.0, false);
    sim.run_until("armed", 600, |s| s.state() == AutomixState::Armed);
    sim.run(10);
    sim.snap.decks[1].position_frames += beat(40.0);
    sim.tick();
    assert_eq!(sim.state(), AutomixState::Idle);
}

#[test]
fn start_begins_the_transition_early_at_the_next_bar() {
    let mut sim = Sim::new(100.0, false);
    sim.run(30);
    assert!(sim.cmds().is_empty());
    sim.control
        .set_param(&ParamId::Automix(AutomixParam::Start), 1.0);
    sim.run_until("transition", 400, |s| {
        s.state() == AutomixState::Transitioning
    });
    let start = sim.ctrl.plan().unwrap().start_beat;
    assert!((100.0..120.0).contains(&start), "{start}");
    sim.run_until("done", 3000, |s| s.states.contains(&AutomixState::Cleanup));
    assert_eq!(sim.xfader(), 1.0);
}

#[test]
fn after_a_transition_the_stopped_deck_gets_the_next_track() {
    let mut sim = Sim::new(380.0, false);
    sim.run_until("cleanup", 3000, |s| {
        s.states.contains(&AutomixState::Cleanup)
    });
    sim.run(5);
    sim.host.tracks.insert(8, info(0, BPM, "8A", false));
    sim.host.queue.push(NextTrack {
        id: 8,
        title: "Third".into(),
        bpm: None,
        key: String::new(),
    });
    // The new leader (deck 1) nears its own end.
    sim.snap.decks[1].position_frames = beat(390.0);
    sim.run_until("load on deck 0", 600, |s| s.host.starts.len() == 2);
    assert_eq!(sim.host.starts[1], (0, 8));
}

#[test]
fn control_target_maps_the_automix_ids() {
    let c = AutomixControl::new();
    assert!(!c.is_enabled());
    assert!(c.set_param(&ParamId::Automix(AutomixParam::Enable), 1.0));
    assert!(c.is_enabled());
    assert!(c.set_param(&ParamId::Automix(AutomixParam::Enable), 0.0));
    assert!(!c.is_enabled());
    for style in AutomixStyle::ALL {
        c.set_param(
            &ParamId::Automix(AutomixParam::Style),
            f64::from(style.as_u8()),
        );
        assert_eq!(c.style(), style);
    }
    c.set_param(&ParamId::Automix(AutomixParam::Style), 99.0);
    assert_eq!(c.style(), AutomixStyle::Auto);
    assert!(c.set_param(&ParamId::Automix(AutomixParam::Start), 0.0));
    assert!(!c.take_start(), "release is not a press");
    c.set_param(&ParamId::Automix(AutomixParam::Start), 1.0);
    assert!(c.take_start() && !c.take_start());
    c.set_param(&ParamId::Automix(AutomixParam::Cancel), 1.0);
    assert!(c.take_cancel() && !c.take_cancel());
    assert!(!c.set_param(&"deck.0.play".parse().unwrap(), 1.0));
}

/// A track loaded before its analysis finishes has no grid yet. The controller must not keep that
/// grid-less copy: once the grid lands, the next ticks arm the transition.
#[test]
fn a_grid_that_lands_after_the_load_is_read_on_a_later_tick() {
    let mut sim = Sim::new(380.0, false);
    let mut analysing = sim.host.infos[&0].clone();
    analysing.grid = None;
    sim.host.infos.insert(0, analysing);
    sim.run(20);
    assert_eq!(sim.state(), AutomixState::Idle, "no grid, no transition");
    sim.host.infos.insert(0, info(0, BPM, "8A", false));
    sim.run_until("armed", 600, |s| s.state() == AutomixState::Armed);
}

/// The transition runs to the end of the outgoing track. The engine stops that deck at its last
/// frame, which the controller must read as the end, not as the user pausing the deck.
#[test]
fn natural_end_of_the_outgoing_track_completes_the_transition() {
    let cfg = AutomixConfig {
        length_beats: Some(32.0),
        ..AutomixConfig::default()
    };
    let mut sim = Sim::with(cfg, 380.0, false, info(1, BPM, "8A", false));
    sim.host.infos.get_mut(&0).unwrap().length_beats = 432.0;
    sim.snap.decks[0].length_frames = beat(432.0);
    sim.run_until("transitioning", 3000, |s| {
        s.state() == AutomixState::Transitioning
    });
    assert_eq!(sim.ctrl.plan().unwrap().end_beat(), 432.0);
    sim.run_until("the track is at its last beat", 3000, |s| {
        s.snap.decks[0].position_frames >= beat(431.5)
    });
    // The engine reaches the end of the track: the deck stops on its last frame.
    sim.snap.decks[0].playing = false;
    sim.snap.decks[0].position_frames = beat(432.0);
    sim.run_until("cleanup", 10, |s| s.states.contains(&AutomixState::Cleanup));
    assert!(
        sim.ctrl.snapshot().last_error.is_empty(),
        "{:?}",
        sim.ctrl.snapshot().last_error
    );
    sim.run_until("idle", 10, |s| s.state() == AutomixState::Idle);
    assert_eq!(sim.xfader(), 1.0, "the crossfader lands on the incoming deck");
    assert!(sim.snap.decks[1].playing && sim.snap.decks[1].sync_master);
    assert!(!sim.snap.decks[0].playing);
}

/// `automix.start` pressed when nothing plays must not wait for the next deck to start playing and
/// then fire a transition the user did not ask for.
#[test]
fn start_pressed_with_nothing_playing_does_not_fire_later() {
    let mut sim = Sim::new(100.0, false);
    sim.snap.decks[0].playing = false;
    sim.control
        .set_param(&ParamId::Automix(AutomixParam::Start), 1.0);
    sim.run(5);
    assert!(sim.host.starts.is_empty());
    // Much later the user starts a track from its beginning.
    sim.snap.decks[0].playing = true;
    sim.snap.decks[0].position_frames = beat(10.0);
    sim.run(200);
    assert!(sim.host.starts.is_empty(), "{:?}", sim.host.starts);
    assert_eq!(sim.state(), AutomixState::Idle);
}

/// A press during a transition is for that transition only: it must not start the track after it.
#[test]
fn start_pressed_during_a_transition_does_not_chain_into_the_next_one() {
    let mut sim = Sim::new(380.0, false);
    sim.host.queue.push(NextTrack {
        id: 8,
        title: "Third".into(),
        bpm: None,
        key: String::new(),
    });
    sim.host.tracks.insert(8, info(0, BPM, "8A", false));
    sim.run_into_transition();
    sim.control
        .set_param(&ParamId::Automix(AutomixParam::Start), 1.0);
    sim.run_until("cleanup", 3000, |s| {
        s.states.contains(&AutomixState::Cleanup)
    });
    sim.run_until("idle", 10, |s| s.state() == AutomixState::Idle);
    sim.run(200);
    assert_eq!(sim.host.starts.len(), 1, "{:?}", sim.host.starts);
}

/// After automix stops a deck, the user loads a different track on it. That track is theirs: it
/// must not be loaded over, and it is mixed into as it is.
#[test]
fn a_track_the_user_loads_over_the_stopped_deck_is_not_loaded_over() {
    let mut sim = Sim::new(380.0, false);
    sim.run_until("cleanup", 3000, |s| {
        s.states.contains(&AutomixState::Cleanup)
    });
    sim.run_until("idle", 10, |s| s.state() == AutomixState::Idle);
    sim.host.queue.push(NextTrack {
        id: 8,
        title: "Third".into(),
        bpm: None,
        key: String::new(),
    });
    sim.host.tracks.insert(8, info(0, BPM, "8A", false));
    // The user loads a shorter track on the stopped deck 0 and starts the deck 1 leader near its end.
    sim.snap.decks[0].length_frames = beat(LENGTH_BEATS - 50.0);
    sim.snap.decks[0].position_frames = 0;
    sim.snap.decks[0].loaded = true;
    sim.snap.decks[1].position_frames = beat(390.0);
    sim.run(100);
    assert!(
        sim.host.starts.iter().all(|(deck, _)| *deck != 0),
        "{:?}",
        sim.host.starts
    );
}
