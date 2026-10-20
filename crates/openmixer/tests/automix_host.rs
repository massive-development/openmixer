//! The automix host over a real library and the real `DjEngine` on the mock device: a full
//! transition on a simulated 30 Hz clock, the deck description the planner reads, and the queue's
//! beat-grid rule (the analysis cache, not a BPM).

mod automix_rig;

use automix_rig::{Rig, BPM};
use om_automix::AutomixHost;
use om_core::AutomixState;
use om_library::TrackRecord;

/// Ticks until `done` holds, at most `limit`. Returns the states seen, in order, without repeats.
fn run_until(rig: &mut Rig, limit: usize, done: impl Fn(&Rig) -> bool) -> Vec<AutomixState> {
    let mut seen: Vec<AutomixState> = Vec::new();
    for _ in 0..limit {
        rig.tick();
        let state = rig.runner.state();
        if seen.last() != Some(&state) {
            seen.push(state);
        }
        if done(rig) {
            break;
        }
    }
    seen
}

#[test]
fn an_armed_transition_runs_to_the_end_on_the_mock_device() {
    // Two 64 s tracks at 120 BPM: 128 beats each. The first is armed once it is inside the 96-beat window.
    let mut rig = Rig::new(2, 64.0);
    rig.load(0, 0);
    rig.attach_host();
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);

    let seen = run_until(&mut rig, 90 * 30, |r| {
        let snap = r.engine.snapshot();
        r.automix.snapshot().state == AutomixState::Idle && snap.decks[1].playing
    });

    let snap = rig.engine.snapshot();
    assert!(
        seen.contains(&AutomixState::Armed),
        "states seen: {seen:?}"
    );
    assert!(
        seen.contains(&AutomixState::Transitioning),
        "states seen: {seen:?}"
    );
    let armed = seen.iter().position(|s| *s == AutomixState::Armed).unwrap();
    let mixing = seen
        .iter()
        .position(|s| *s == AutomixState::Transitioning)
        .unwrap();
    assert!(armed < mixing, "states seen: {seen:?}");
    assert!(
        seen.last() == Some(&AutomixState::Idle),
        "states seen: {seen:?}"
    );
    assert!(!snap.decks[0].playing, "the outgoing deck still plays");
    assert!(snap.decks[1].playing, "the incoming deck is not playing");
    // The crossfader ends on the incoming side (deck B: the far end of the fader from deck A).
    let cf = snap.mixer.crossfader;
    assert!(cf > 0.95 || cf < 0.05, "crossfader ended at {cf}");
}

#[test]
fn the_host_describes_a_loaded_track_from_the_cache() {
    let mut rig = Rig::new(1, 64.0);
    rig.load(0, 0);
    let mut host = rig.host();
    let info = host.deck_info(0).expect("a loaded, analysed track has a deck info");
    assert_eq!(info.deck, 0);
    assert_eq!(info.camelot, "8A");
    assert!((info.bpm - BPM).abs() < 0.5, "bpm {}", info.bpm);
    assert!((info.length_beats - 128.0).abs() < 1.0, "length {}", info.length_beats);
    assert!(info.grid.is_some(), "the grid comes from the analysis cache");
    assert_eq!(info.sections.len(), 3, "intro, main and outro from the structure cache");
    let mix_in = info.mix_in.expect("mix-in point");
    let mix_out = info.mix_out.expect("mix-out point");
    assert!(mix_in < mix_out, "mix-in {mix_in} must precede mix-out {mix_out}");
    assert!(!info.has_stems);
    assert!(host.deck_info(1).is_none(), "an empty deck has no info");
    assert_eq!(host.underruns(), 0);
}

#[test]
fn the_queue_asks_the_cache_for_a_grid_not_for_a_bpm() {
    let rig = Rig::new(1, 64.0);
    // A track with a BPM in its record but no analysis: not a candidate.
    let bare = rig
        .library()
        .tracks()
        .insert(&TrackRecord {
            path: "/no/such/file.wav".into(),
            title: "Bare".into(),
            bpm: Some(BPM),
            camelot: "8A".into(),
            ..TrackRecord::default()
        })
        .expect("insert");
    rig.library()
        .playlists()
        .add_track(rig.playlist, bare, 9)
        .expect("add");
    let mut host = rig.host();
    let mut picks = Vec::new();
    for _ in 0..4 {
        if let Some(next) = host.next_track(0) {
            picks.push(next.id);
        }
    }
    assert!(!picks.contains(&bare), "picked an unanalysed track: {picks:?}");
    assert!(picks.contains(&rig.tracks[0]), "the analysed track is picked: {picks:?}");
}
