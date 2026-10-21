//! Automix over stem-mode decks, through the real `DjEngine` on the mock device: a bass swap moves
//! the stem volumes of both decks, and decks without stems fall back to a plain crossfade.

mod automix_rig;

use automix_rig::{Rig, RATE};
use om_core::{AutomixState, AutomixStyle, Command, StateId, StemSetRef};
use om_testing::wav::write_pcm16_wav;
use std::path::Path;

const STEM_FILES: [&str; 4] = ["drums.wav", "bass.wav", "vocals.wav", "other.wav"];
const BASS: usize = 1;

/// A stem set as long as the rig's tracks (silent: only the stem controls are under test).
fn write_stems(dir: &Path, seconds: f64) {
    let frames = (seconds * f64::from(RATE)) as usize;
    let silence = vec![0_i16; frames * 2];
    for name in STEM_FILES {
        write_pcm16_wav(&dir.join(name), 2, RATE, &silence);
    }
}

fn load_stems(rig: &Rig, set: StemSetRef, deck: u8) {
    let sink = rig.engine.command_sink(None);
    sink.send(Command::LoadStems { deck, set }).expect("stems");
}

fn volume(rig: &Rig, deck: usize, stem: usize) -> f64 {
    let id: StateId = format!("stem.{deck}.{stem}.volume").parse().unwrap();
    rig.engine.snapshot().read(&id).expect("stem volume")
}

#[test]
fn a_bass_swap_moves_both_decks_bass_stems_and_ends_on_the_incoming_deck() {
    let mut rig = Rig::new(2, 64.0);
    let stems = tempfile::tempdir().expect("tempdir");
    write_stems(stems.path(), 64.0);
    let set = rig.engine.register_stem_set(stems.path());

    rig.load(0, 0);
    load_stems(&rig, set, 0);
    rig.attach_host();
    rig.set("automix.style", f64::from(AutomixStyle::BassSwap.as_u8()));
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);

    // The incoming deck is loaded once armed; give it stems before the transition is planned.
    let mut loaded_stems = false;
    let mut low_a = f64::MAX;
    let mut low_b = f64::MAX;
    let mut transitioned = false;
    for _ in 0..120 * 30 {
        rig.tick();
        let state = rig.automix.snapshot().state;
        if !loaded_stems && rig.engine.deck(1).is_loaded() {
            load_stems(&rig, set, 1);
            loaded_stems = true;
        }
        if state == AutomixState::Transitioning {
            transitioned = true;
            low_a = low_a.min(volume(&rig, 0, BASS));
            low_b = low_b.min(volume(&rig, 1, BASS));
        }
        if transitioned && state == AutomixState::Idle {
            break;
        }
    }
    assert!(loaded_stems, "the incoming deck was never loaded");
    assert!(transitioned, "no transition ran");
    assert!(low_b < 0.2, "the incoming bass was never cut: {low_b}");
    assert!(low_a < 0.2, "the outgoing bass never went down: {low_a}");
    let high_b = volume(&rig, 1, BASS);
    assert!(high_b > 0.8, "the incoming bass did not come up: {high_b}");
    assert!(
        rig.automix.snapshot().last_error.is_empty(),
        "{:?}",
        rig.automix.snapshot().last_error
    );
}

#[test]
fn a_stem_bridge_leaves_only_the_outgoing_drums_for_the_second_half() {
    let mut rig = Rig::new(2, 64.0);
    let stems = tempfile::tempdir().expect("tempdir");
    write_stems(stems.path(), 64.0);
    let set = rig.engine.register_stem_set(stems.path());

    rig.load(0, 0);
    load_stems(&rig, set, 0);
    rig.attach_host();
    rig.set("automix.style", f64::from(AutomixStyle::StemBridge.as_u8()));
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);

    let mut loaded_stems = false;
    let mut transitioned = false;
    let mut vocals_cut = false;
    for _ in 0..120 * 30 {
        rig.tick();
        let state = rig.automix.snapshot().state;
        if !loaded_stems && rig.engine.deck(1).is_loaded() {
            load_stems(&rig, set, 1);
            loaded_stems = true;
        }
        if state == AutomixState::Transitioning {
            transitioned = true;
            let stems = &rig.engine.snapshot().stems[0];
            vocals_cut |= stems[2].muted || stems[2].gain < 0.2;
            assert!(!stems[0].muted, "the drums must keep playing");
        }
        if transitioned && state == AutomixState::Idle {
            break;
        }
    }
    assert!(transitioned, "no transition ran");
    assert!(vocals_cut, "the outgoing vocals were never taken out");
}

#[test]
fn decks_without_stems_fall_back_to_a_crossfade_and_leave_the_stem_controls_alone() {
    let mut rig = Rig::new(2, 64.0);
    rig.load(0, 0);
    rig.attach_host();
    rig.set("automix.style", f64::from(AutomixStyle::BassSwap.as_u8()));
    rig.set("deck.0.play", 1.0);
    rig.set("automix.enable", 1.0);

    let mut transitioned = false;
    for _ in 0..120 * 30 {
        rig.tick();
        let state = rig.automix.snapshot().state;
        transitioned |= state == AutomixState::Transitioning;
        if transitioned && state == AutomixState::Idle {
            break;
        }
    }
    assert!(transitioned, "no transition ran");
    assert_eq!(volume(&rig, 0, BASS), 1.0);
    assert_eq!(volume(&rig, 1, BASS), 1.0);
    assert!(rig.engine.snapshot().decks[1].playing);
}
