//! Ports of the loop, slip, quantize and sync cases of tests/unit/EngineWiringTest.cpp, driven
//! without the engine: each deck gets its own `DeckAudio` and `Sync::apply` stands in for the
//! control timer. Same numbers and tolerances as the C++.

use super::DeckControl;
use crate::deck::tests::{pump_until_buffered, wait_for, write_float_wav, Fixture, RATE};
use crate::deck::{Deck, DeckAudio};
use crate::grid::FixedTempoGrid;
use crate::sync::{Sync, SyncDeckState};
use om_core::ParameterTarget;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Frames in the 10-second 220 Hz sine fixture (C++ `kFrames`).
const SINE_FRAMES: usize = 44_100 * 10;

/// Writes a stereo 220 Hz sine, 0.3 amplitude, at the test sample rate.
fn sine_wav(path: &Path) -> PathBuf {
    let mut samples = Vec::with_capacity(SINE_FRAMES * 2);
    for i in 0..SINE_FRAMES {
        let phase = 2.0 * std::f64::consts::PI * 220.0 * i as f64 / RATE;
        let v = (0.3 * phase.sin()) as f32;
        samples.push(v);
        samples.push(v);
    }
    write_float_wav(path, 2, &samples);
    path.to_path_buf()
}

/// Renders blocks until the deck sits on `target`, servicing its seek handshake. Bounded.
fn settle_at(deck: &Deck, audio: &mut DeckAudio, target: i64) -> bool {
    let mut l = [0.0_f32; 64];
    let mut r = [0.0_f32; 64];
    wait_for(
        || {
            audio.render_stereo(&mut l, &mut r);
            deck.position_frames() == target
        },
        Duration::from_secs(20),
    )
}

/// Decks with their controls, all sharing one `Sync`. Audio is returned separately so tests can
/// render it.
struct Rig {
    sync: Arc<Sync>,
    decks: Vec<Arc<Deck>>,
    controls: Vec<DeckControl>,
}

impl Rig {
    fn new(num_decks: usize) -> (Self, Vec<DeckAudio>) {
        let sync = Arc::new(Sync::new(num_decks));
        let mut decks = Vec::new();
        let mut controls = Vec::new();
        let mut audio = Vec::new();
        for index in 0..num_decks {
            let (deck, mut deck_audio) = Deck::new(1 << 16);
            deck_audio.prepare(RATE, 256);
            let deck = Arc::new(deck);
            controls.push(DeckControl::new(
                Arc::clone(&deck),
                Arc::clone(&sync),
                index,
            ));
            decks.push(deck);
            audio.push(deck_audio);
        }
        (
            Self {
                sync,
                decks,
                controls,
            },
            audio,
        )
    }

    fn deck_refs(&self) -> Vec<Option<&Deck>> {
        self.decks.iter().map(|d| Some(&**d)).collect()
    }
}

#[test]
fn deck_ids_still_reach_the_deck_and_unknown_ids_fail() {
    let (rig, _audio) = Rig::new(1);
    let control = &rig.controls[0];
    assert!(control.set_parameter("rate", 1.05));
    assert_eq!(rig.decks[0].rate(), 1.05);
    assert!(!control.set_parameter("bogus", 1.0));
    for id in [
        "loop.quantized.abc",
        "loop.quantized.",
        "loop.quantized.0",
        "loop.quantized.1.2.3",
        "loop.quantized..5",
    ] {
        assert!(!control.set_parameter(id, 1.0), "{id}");
    }
    assert!(
        !control.set_parameter("loop.quantized.2", 1.0),
        "no grid: press reports failure"
    );
    assert!(
        control.set_parameter("loop.quantized.2", 0.0),
        "release is a no-op"
    );
}

#[test]
fn sync_ids_select_leader_and_follower_and_match_tempo() {
    let (rig, _audio) = Rig::new(2);
    rig.controls[0].set_grid(Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
    rig.controls[1].set_grid(Some(Arc::new(FixedTempoGrid::new(100.0, RATE, 0))));
    assert!(rig.controls[0].set_parameter("sync.master", 1.0));
    assert!(rig.controls[1].set_parameter("sync", 1.0));
    assert_eq!(rig.sync.leader(), Some(0));
    assert!(rig.sync.is_enabled(1));

    // No audio callback runs: positions stay at 0, so phase is aligned.
    rig.decks[0].play();
    rig.decks[1].play();
    rig.sync.apply(&rig.deck_refs());
    assert!(
        (rig.decks[1].rate() - 1.2).abs() < 1e-9,
        "100 bpm deck sped up to the 120 bpm leader"
    );
    assert_eq!(rig.decks[0].rate(), 1.0);

    assert!(rig.controls[1].set_parameter("sync", 0.0));
    assert!(
        rig.controls[1].set_parameter("sync.master", 0.0),
        "releasing a non-leader is a no-op"
    );
    assert_eq!(rig.sync.leader(), Some(0));
    rig.controls[0].set_parameter("sync.master", 0.0);
    assert_eq!(rig.sync.leader(), None);
}

#[test]
fn sync_aligns_two_decks_on_fixed_grids() {
    let dir = Fixture::new();
    let file = sine_wav(&dir.path("sine.wav"));
    let (rig, mut audio) = Rig::new(2);
    for deck in &rig.decks {
        deck.load(&file).expect("sine loads");
    }
    rig.controls[0].set_grid(Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
    rig.controls[1].set_grid(Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));

    // Follower starts 0.1 beat late.
    rig.decks[1].seek(2205);
    assert!(settle_at(&rig.decks[1], &mut audio[1], 2205));
    for (deck, deck_audio) in rig.decks.iter().zip(audio.iter_mut()) {
        let pos = deck.position_frames();
        assert!(pump_until_buffered(deck, deck_audio, 8192, pos));
    }
    assert!(rig.controls[0].set_parameter("sync.master", 1.0));
    assert!(rig.controls[1].set_parameter("sync", 1.0));
    rig.decks[0].play();
    rig.decks[1].play();

    let mut l = [0.0_f32; 256];
    let mut r = [0.0_f32; 256];
    for block in 0..1200 {
        assert!(wait_for(
            || rig.decks.iter().all(|d| d.buffered_frames() >= 2048),
            Duration::from_secs(10)
        ));
        for deck_audio in audio.iter_mut() {
            deck_audio.render_stereo(&mut l, &mut r);
        }
        if block % 8 == 0 {
            rig.sync.apply(&rig.deck_refs());
        }
    }
    let states: Vec<SyncDeckState> = rig
        .decks
        .iter()
        .map(|d| SyncDeckState {
            position: d.position_frames(),
            rate: d.rate(),
            playing: true,
        })
        .collect();
    let phase = rig.sync.phase_error(&states, 1);
    assert!(
        phase.abs() < 0.02,
        "follower converged onto the leader's beat: {phase}"
    );
}

#[test]
fn quantized_loop_bounds_come_from_the_grid() {
    let dir = Fixture::new();
    let file = sine_wav(&dir.path("sine.wav"));
    let (rig, mut audio) = Rig::new(1);
    rig.decks[0].load(&file).expect("sine loads");
    rig.decks[0].seek(30_000); // beat 1.36 at 120 bpm: nearest beat is 22050
    assert!(settle_at(&rig.decks[0], &mut audio[0], 30_000));

    let control = &rig.controls[0];
    assert!(!control.set_quantized_loop(2.0), "no grid yet");
    control.set_grid(Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
    assert!(control.set_parameter("loop.quantized.2", 1.0));
    let loop_ = rig.decks[0].get_loop();
    assert_eq!(loop_.start, 22_050);
    assert_eq!(loop_.end, 22_050 + 44_100);

    assert!(control.set_parameter("loop.quantized.0.5", 1.0));
    let loop_ = rig.decks[0].get_loop();
    assert_eq!(loop_.end - loop_.start, 11_025, "half-beat loop");
}

#[test]
fn quantize_off_starts_the_loop_exactly_at_the_playhead() {
    let dir = Fixture::new();
    let file = sine_wav(&dir.path("sine.wav"));
    let (rig, mut audio) = Rig::new(1);
    rig.decks[0].load(&file).expect("sine loads");
    rig.decks[0].seek(30_000);
    assert!(settle_at(&rig.decks[0], &mut audio[0], 30_000));
    let control = &rig.controls[0];
    control.set_grid(Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));

    assert!(control.set_parameter("quantize", 0.0));
    assert!(!control.is_quantize());
    assert!(control.set_parameter("loop.quantized.1", 1.0));
    assert_eq!(rig.decks[0].get_loop().start, 30_000);

    assert!(control.set_parameter("quantize", 1.0));
    assert!(control.is_quantize());
}

#[test]
fn slip_resumes_where_the_track_would_have_been() {
    let dir = Fixture::new();
    let file = sine_wav(&dir.path("sine.wav"));
    let (rig, mut audio) = Rig::new(1);
    rig.decks[0].load(&file).expect("sine loads");
    rig.decks[0].seek(1000);
    assert!(settle_at(&rig.decks[0], &mut audio[0], 1000));

    let control = &rig.controls[0];
    assert!(control.set_parameter("slip", 1.0));
    assert!(control.is_slipping());
    rig.decks[0].play(); // no audio callback runs: the playhead is held, the ghost moves
    control.tick(0.5);
    rig.decks[0].pause();
    assert!(control.set_parameter("slip", 0.0));
    assert!(!control.is_slipping());
    assert!(settle_at(&rig.decks[0], &mut audio[0], 1000 + 22_050));
}
