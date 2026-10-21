//! Automix safety (A6): a user's input always wins, a plan stops cleanly, and the controller and
//! planner stay off the audio side. Named by the behaviour they pin; each runs the real engine on
//! the mock device through the shared rig.

mod automix_rig;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use automix_rig::Rig;
use om_automix::{AutomixHost, DeckInfo, LoadPoll, NextTrack};
use om_core::AutomixState;

/// Ticks until the plan is transitioning (at most 90 simulated seconds).
fn until_transitioning(rig: &mut Rig) {
    for _ in 0..90 * 30 {
        rig.tick();
        if rig.automix.snapshot().state == AutomixState::Transitioning {
            return;
        }
    }
    panic!("no transition started: {:?}", rig.automix.snapshot());
}

/// A rig with deck A playing and automix on, parked in a transition.
fn transitioning_rig() -> Rig {
    let mut rig = Rig::new(2, 64.0);
    rig.load(0, 0);
    rig.attach_host();
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);
    until_transitioning(&mut rig);
    rig
}

fn cancelled_with(rig: &Rig, needle: &str) -> bool {
    let snap = rig.automix.snapshot();
    snap.state == AutomixState::Idle && snap.last_error.contains(needle)
}

#[test]
fn a_manual_crossfader_move_cancels_and_the_users_value_wins() {
    let mut rig = transitioning_rig();
    rig.set("mixer.crossfader", 0.2);
    rig.tick();
    assert!(
        cancelled_with(&rig, "crossfader"),
        "{:?}",
        rig.automix.snapshot()
    );
    for _ in 0..30 {
        rig.tick();
    }
    let cf = rig.engine.snapshot().mixer.crossfader;
    assert!(
        (cf - 0.2).abs() < 0.01,
        "the crossfader snapped back to {cf}"
    );
}

#[test]
fn a_manual_play_on_the_outgoing_deck_cancels_and_is_kept() {
    let mut rig = transitioning_rig();
    rig.set("deck.0.play", 0.0);
    rig.tick();
    assert!(
        cancelled_with(&rig, "deck.0.play"),
        "{:?}",
        rig.automix.snapshot()
    );
    for _ in 0..10 {
        rig.tick();
    }
    assert!(
        !rig.engine.snapshot().decks[0].playing,
        "automix restarted the user's pause"
    );
}

#[test]
fn a_manual_seek_on_the_incoming_deck_cancels() {
    let mut rig = transitioning_rig();
    rig.set("deck.1.seek", 0.5);
    rig.tick();
    assert!(
        cancelled_with(&rig, "deck.1.seek"),
        "{:?}",
        rig.automix.snapshot()
    );
}

#[test]
fn a_manual_channel_fader_move_cancels() {
    let mut rig = transitioning_rig();
    rig.set("mixer.ch1.fader", 0.3);
    rig.tick();
    assert!(
        cancelled_with(&rig, "mixer.ch1.fader"),
        "{:?}",
        rig.automix.snapshot()
    );
}

#[test]
fn disabling_mid_transition_stops_cleanly() {
    let mut rig = transitioning_rig();
    rig.set("automix.enable", 0.0);
    for _ in 0..5 {
        rig.tick();
    }
    let snap = rig.automix.snapshot();
    assert!(!snap.enabled);
    assert_eq!(snap.state, AutomixState::Idle);
}

/// Adds a number of underruns on top of the real host's count.
struct Underruns {
    inner: Box<dyn AutomixHost>,
    extra: Arc<AtomicU64>,
}

impl AutomixHost for Underruns {
    fn next_track(&mut self, deck: u8, tempo_gate: Option<f64>) -> Option<NextTrack> {
        self.inner.next_track(deck, tempo_gate)
    }
    fn start_load(&mut self, deck: u8, track: &NextTrack) -> Result<(), String> {
        self.inner.start_load(deck, track)
    }
    fn poll_load(&mut self, deck: u8) -> LoadPoll {
        self.inner.poll_load(deck)
    }
    fn deck_info(&mut self, deck: u8) -> Option<DeckInfo> {
        self.inner.deck_info(deck)
    }
    fn deck_title(&mut self, deck: u8) -> String {
        self.inner.deck_title(deck)
    }
    fn underruns(&self) -> u64 {
        self.inner.underruns() + self.extra.load(Ordering::SeqCst)
    }
}

#[test]
fn an_underrun_during_a_transition_turns_automix_off_with_a_reason() {
    let mut rig = Rig::new(2, 64.0);
    rig.load(0, 0);
    let extra = Arc::new(AtomicU64::new(0));
    let host = Underruns {
        inner: Box::new(rig.host()),
        extra: Arc::clone(&extra),
    };
    rig.runner.set_host(Box::new(host));
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);
    until_transitioning(&mut rig);
    extra.store(3, Ordering::SeqCst);
    rig.tick();
    let snap = rig.automix.snapshot();
    assert!(
        !snap.enabled,
        "automix stayed on after an underrun: {snap:?}"
    );
    assert!(
        snap.last_error.contains("underrun"),
        "no visible reason: {snap:?}"
    );
}

#[test]
fn one_track_is_never_mixed_with_itself() {
    let mut rig = Rig::new(1, 64.0);
    rig.load(0, 0);
    rig.attach_host();
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);
    for _ in 0..90 * 30 {
        rig.tick();
        let s = rig.automix.snapshot().state;
        assert!(
            s != AutomixState::Loading && s != AutomixState::Transitioning,
            "a single track started a transition: {s:?}"
        );
    }
    assert!(!rig.engine.snapshot().decks[1].loaded);
}

#[test]
fn shutdown_while_transitioning_is_clean() {
    let rig = transitioning_rig();
    // Dropping the rig stops the device and the engine mid-transition: no panic, no hang.
    drop(rig);
}

#[test]
fn the_controller_and_planner_have_no_audio_dependency() {
    let manifest = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../om-automix/Cargo.toml"
    ))
    .expect("om-automix manifest");
    for banned in ["om-engine", "om-audio", "cpal", "om-stretch"] {
        assert!(!manifest.contains(banned), "om-automix depends on {banned}");
    }
}

#[test]
fn a_cancel_with_no_plan_is_harmless() {
    let mut rig = Rig::new(2, 64.0);
    rig.set("automix.cancel", 1.0);
    rig.set("automix.enable", 1.0);
    rig.tick();
    assert_eq!(rig.automix.snapshot().state, AutomixState::Idle);
}

/// Tick path (A6): the controller's `tick` gets a copied `EngineSnapshot`, a `CommandSink` and its
/// host, and nothing else. So it cannot take an engine or audio lock: no engine type is in reach
/// and no lock is named in its sources. The window's runner is the only part that reads the
/// engine (through `DjEngine::snapshot`), and it sits outside the controller. This pins the
/// source-level half of that rule; the behaviour tests above run the controller end to end.
#[test]
fn the_controller_tick_path_takes_no_engine_or_audio_locks() {
    let dir = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../om-automix/src/controller"
    ));
    let banned = [
        "Mutex",
        "RwLock",
        ".lock()",
        "DjEngine",
        "om_engine",
        "om_audio",
        "cpal",
    ];
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).expect("controller sources") {
        let path = entry.expect("directory entry").path();
        // The unit tests are not the tick path; the rest of the controller is.
        if path.extension().is_none_or(|e| e != "rs") || path.ends_with("tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable source");
        for name in banned {
            assert!(
                !text.contains(name),
                "{} names {name}: the controller tick path must take no lock and reach no engine",
                path.display()
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 4,
        "the controller sources were not found ({checked} files)"
    );
}
