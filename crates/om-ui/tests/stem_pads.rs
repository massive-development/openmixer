//! Stem pad model over the engine: id round trip, solo with restore, the all-on reset, and the
//! activity level. The deck plays a real stereo WAV with a four-stem set beside it.

use std::path::Path;
use std::sync::Arc;

use om_core::param_id::StemParam;
use om_core::{ParamId, ParameterRouter, ParameterTarget};
use om_engine::DjEngine;
use om_testing::signals::sine_amp;
use om_testing::wav::write_pcm16_wav;
use om_ui::stem_binding::{StemBinding, MAX_VOLUME};

const RATE: u32 = 44_100;
const FRAMES: usize = RATE as usize; // one second

/// A stereo 16-bit file at `freq`, `FRAMES` long.
fn write_stereo(path: &Path, freq: f64) {
    let mono = sine_amp(freq, 0.25, f64::from(RATE), FRAMES);
    let interleaved: Vec<i16> = mono
        .iter()
        .flat_map(|s| {
            let v = (s * 32767.0) as i16;
            [v, v]
        })
        .collect();
    write_pcm16_wav(path, 2, RATE, &interleaved);
}

/// A deck-0 rig: the main file loaded, and the four stems loaded into the deck.
fn stem_rig(dir: &Path) -> (Arc<DjEngine>, Arc<ParameterRouter>, Box<dyn std::any::Any>) {
    let (engine, audio) = DjEngine::new(2);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let main = dir.join("main.wav");
    write_stereo(&main, 220.0);
    engine.deck(0).load(&main).expect("main loads");
    let stems = dir.join("stems");
    std::fs::create_dir_all(&stems).unwrap();
    for (i, name) in ["drums", "bass", "vocals", "other"].iter().enumerate() {
        write_stereo(&stems.join(format!("{name}.wav")), 110.0 * (i + 1) as f64);
    }
    engine.deck(0).load_stems(&stems).expect("stems load");
    (engine, router, Box::new(audio))
}

#[test]
fn stem_ids_round_trip_through_their_text_form() {
    let (engine, _audio) = DjEngine::new(2);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    let binding = StemBinding::typed(engine, router, 1);
    for (stem, p, text) in [
        (0, StemParam::Volume, "stem.1.0.volume"),
        (2, StemParam::Mute, "stem.1.2.mute"),
        (3, StemParam::Filter, "stem.1.3.filter"),
    ] {
        let id = binding.id(stem, p).expect("valid id");
        assert_eq!(id.to_string(), text);
        assert_eq!(text.parse::<ParamId>().unwrap(), id);
    }
    assert!(
        binding.id(4, StemParam::Mute).is_err(),
        "stem 4 does not exist"
    );
}

#[test]
fn solo_plays_one_stem_and_the_second_press_restores_the_mutes() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(Arc::clone(&engine), router, 0);
    assert!(pads.has_stems());

    assert!(pads.toggle_mute(1), "bass is muted before the solo");
    assert!(pads.solo(2));
    let muted: Vec<bool> = pads.stems().iter().map(|s| s.muted).collect();
    assert_eq!(muted, [true, true, false, true]);
    assert_eq!(pads.soloed(), Some(2));

    assert!(pads.solo(2));
    let muted: Vec<bool> = pads.stems().iter().map(|s| s.muted).collect();
    assert_eq!(
        muted,
        [false, true, false, false],
        "the mutes from before the solo come back"
    );
    assert_eq!(pads.soloed(), None);
}

#[test]
fn a_mute_during_a_solo_is_kept_for_the_restore() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(engine, router, 0);
    assert!(pads.solo(0));
    assert!(pads.set_mute(3, true));
    assert!(pads.solo(0));
    let muted: Vec<bool> = pads.stems().iter().map(|s| s.muted).collect();
    assert_eq!(muted, [false, false, false, true]);
}

#[test]
fn all_on_clears_solo_mutes_gains_and_filters() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(engine, router, 0);
    pads.solo(1);
    assert!(pads.set_volume(2, 9.0));
    assert!(pads.set_filter(0, -5.0));
    assert!(
        (pads.stems()[2].gain - MAX_VOLUME as f32).abs() < 1e-6,
        "volume clamps to 1.5"
    );
    assert!(
        (pads.stems()[0].filter + 1.0).abs() < 1e-6,
        "filter clamps to -1"
    );

    assert!(pads.all_on());
    assert_eq!(pads.soloed(), None);
    for s in pads.stems() {
        assert!(!s.muted);
        assert!((s.gain - 1.0).abs() < 1e-6);
        assert!(s.filter.abs() < 1e-6);
    }
}

#[test]
fn level_is_zero_when_muted_and_follows_gain_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(engine, router, 0);
    assert!((pads.level(0) - 1.0 / 1.5).abs() < 1e-6);
    pads.toggle_mute(0);
    assert_eq!(pads.level(0), 0.0);
}

#[test]
fn a_deck_without_stems_has_no_pads_enabled() {
    let (engine, _audio) = DjEngine::new(2);
    let pads = StemBinding::new(Arc::new(engine), Arc::new(ParameterRouter::new()), 0);
    assert!(!pads.has_stems());
    assert_eq!(pads.level(0), 0.0);
}

/// The engine's stem mutes as the pads read them.
fn mutes(pads: &StemBinding) -> Vec<bool> {
    pads.stems().iter().map(|s| s.muted).collect()
}

/// Sets a mute through the router, as a MIDI controller would (bypassing the pads).
fn midi_mute(router: &ParameterRouter, stem: u8, muted: bool) {
    let id = ParamId::Stem {
        deck: 0,
        stem,
        p: StemParam::Mute,
    };
    assert!(router.set_parameter(&id.to_string(), if muted { 1.0 } else { 0.0 }));
}

#[test]
fn solo_survives_clear_stems_only_until_the_deck_leaves_stem_mode() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(Arc::clone(&engine), router, 0);
    pads.observe_track(Some(1));
    assert!(pads.solo(2));
    assert_eq!(pads.soloed(), Some(2));

    engine.deck(0).clear_stems();
    pads.observe_track(Some(1));
    assert!(!pads.has_stems());
    assert_eq!(pads.soloed(), None, "the solo ends with the stems");

    let stems = dir.path().join("stems");
    engine.deck(0).load_stems(&stems).expect("stems reload");
    pads.observe_track(Some(1));
    assert!(pads.solo(2));
    assert_eq!(pads.soloed(), Some(2), "solo again on the reloaded stems");
    assert!(pads.solo(2));
    assert_eq!(
        mutes(&pads),
        [true, true, false, true],
        "the mutes the engine kept from the first solo are what the restore brings back"
    );
}

#[test]
fn solo_ends_when_another_track_loads_with_its_own_stems() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(Arc::clone(&engine), router, 0);
    pads.observe_track(Some(1));
    assert!(pads.solo(1));

    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    for name in ["drums", "bass", "vocals", "other"] {
        write_stereo(&other.join(format!("{name}.wav")), 330.0);
    }
    engine.deck(0).clear_stems();
    engine.deck(0).load_stems(&other).expect("other stems load");
    pads.observe_track(Some(2));
    assert!(pads.has_stems());
    assert_eq!(pads.soloed(), None, "a new track starts with no solo");
    assert!(pads.solo(3));
    assert_eq!(pads.soloed(), Some(3));
    assert!(pads.solo(3));
    assert_eq!(
        mutes(&pads),
        [true, false, true, true],
        "the restore is what the engine held when the new solo began, not the old solo's saved mutes"
    );
}

#[test]
fn a_midi_mute_during_a_solo_ends_the_solo() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(Arc::clone(&engine), Arc::clone(&router), 0);
    pads.observe_track(Some(1));
    assert!(pads.solo(2));
    midi_mute(&router, 2, true);
    assert_eq!(pads.soloed(), None, "the engine no longer shows the solo");

    midi_mute(&router, 2, false);
    assert!(pads.solo(0));
    assert_eq!(pads.soloed(), Some(0));
    assert!(pads.solo(0));
    assert_eq!(
        mutes(&pads),
        [true, true, false, true],
        "the restore brings back the mutes the engine showed when the new solo began"
    );
}

#[test]
fn a_second_solo_moves_the_solo_and_keeps_the_first_restore() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, router, _audio) = stem_rig(dir.path());
    let pads = StemBinding::typed(engine, router, 0);
    assert!(pads.toggle_mute(1));
    assert!(pads.solo(2));
    assert!(pads.solo(3));
    assert_eq!(pads.soloed(), Some(3));
    assert_eq!(mutes(&pads), [true, true, true, false]);
    assert!(pads.solo(3));
    assert_eq!(mutes(&pads), [false, true, false, false]);
    assert_eq!(pads.soloed(), None);
}
