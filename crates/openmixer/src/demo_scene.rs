//! DEMO / TEST ONLY (`--demo-scene=NAME`, see `flags::DemoScene`). Stages deck A's engine state
//! for screenshots: hot cues, a loop, playback, a hot channel trim. Nothing here is a product
//! behaviour and nothing is saved; the numbers are demo signal values, not design values.

use om_engine::quantizer::Loop;
use om_engine::DjEngine;
use std::time::{Duration, Instant};

use crate::flags::DemoScene;

/// Hot cue positions of the `cues` scene, in seconds. Pads 6 to 8 stay empty so the capture
/// shows both a set and an empty pad.
const CUE_SECONDS: [f64; 5] = [0.0, 0.25, 1.0, 1.5, 1.75];
/// The `cues` loop starts on cue 3 and is shorter than the window in which the deck marks a pad
/// active (0.25 s after the cue, `deck_view::ACTIVE_WINDOW_S`), so the playhead always sits in
/// that window and pad 3 is steadily ACTIVE in the capture.
const LOOP_SECONDS: (f64, f64) = (1.0, 1.2);
/// Where the `cues` playhead starts: just inside the loop, so the hatch is under it.
const CUES_SEEK_SECONDS: f64 = 1.05;
/// Linear trim of the `meters` scene. The demo tracks peak at 0.25, so 4.5 pushes the channel's
/// peak past the theme's clip level (0.99) and the clip lamp latches.
const HOT_TRIM: f32 = 4.5;
/// Where the `platter` scene starts, in seconds.
const PLATTER_START_SECONDS: f64 = 8.0;
/// Where the `track` scene starts, as a fraction of the track (past the intro).
const TRACK_START: f64 = 0.25;

/// What to set on deck A once its track is loaded.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeckPlan {
    /// Hot cues as (pad, source frame).
    pub cues: Vec<(usize, i64)>,
    pub looping: Option<Loop>,
    pub seek: Option<i64>,
    pub trim: Option<f32>,
    pub play: bool,
}

/// The plan for `scene` on a track of `length` frames at `rate` Hz.
pub fn deck_plan(scene: DemoScene, length: i64, rate: f64) -> DeckPlan {
    let at = |seconds: f64| ((seconds * rate) as i64).clamp(0, length.max(0));
    match scene {
        DemoScene::Cues => DeckPlan {
            cues: CUE_SECONDS
                .iter()
                .enumerate()
                .map(|(pad, s)| (pad, at(*s)))
                .collect(),
            looping: Some(Loop {
                start: at(LOOP_SECONDS.0),
                end: at(LOOP_SECONDS.1),
            }),
            seek: Some(at(CUES_SEEK_SECONDS)),
            trim: None,
            play: true,
        },
        // A looped whole track: the 2 s demo tracks would otherwise end before the capture.
        DemoScene::Meters => DeckPlan {
            looping: Some(Loop {
                start: 0,
                end: length,
            }),
            seek: Some(0),
            trim: Some(HOT_TRIM),
            play: true,
            ..DeckPlan::default()
        },
        // Plays on from a few seconds in, so the platter's position ring has an arc to show.
        DemoScene::Platter => DeckPlan {
            seek: Some(at(PLATTER_START_SECONDS)),
            play: true,
            ..DeckPlan::default()
        },
        DemoScene::Track => DeckPlan {
            seek: Some((length as f64 * TRACK_START) as i64),
            play: true,
            ..DeckPlan::default()
        },
        DemoScene::Sampler | DemoScene::Stems => DeckPlan::default(),
    }
}

/// Applies `scene` to deck A: waits (bounded) for the track's length, then sets the plan.
pub fn apply(scene: DemoScene, engine: &DjEngine) {
    let deck = engine.deck(0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while deck.length_frames() <= 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let plan = deck_plan(scene, deck.length_frames(), deck.file_sample_rate());
    for (pad, frame) in &plan.cues {
        deck.set_hotcue(*pad, *frame);
    }
    if let Some(looping) = plan.looping {
        deck.set_loop(looping);
    }
    if let Some(frame) = plan.seek {
        deck.seek(frame);
    }
    if let Some(trim) = plan.trim {
        engine.mixer().set_trim(0, trim);
    }
    if plan.play {
        deck.play();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;
    const LEN: i64 = 88_200;

    #[test]
    fn cues_scene_sets_five_cues_and_a_loop_under_the_playhead() {
        let plan = deck_plan(DemoScene::Cues, LEN, RATE);
        assert_eq!(plan.cues.len(), 5, "pads 6 to 8 stay empty");
        assert!(plan.cues.iter().all(|(_, f)| (0..=LEN).contains(f)));
        let looping = plan.looping.expect("a loop");
        assert!(looping.valid() && looping.end <= LEN);
        let seek = plan.seek.expect("a seek");
        assert!((looping.start..looping.end).contains(&seek));
        assert!(
            plan.cues.iter().any(|(_, f)| *f == looping.start),
            "the loop starts on a cue, so that pad is active while it plays"
        );
        assert!(looping.end - looping.start < (0.25 * RATE) as i64);
        assert!(plan.play);
    }

    #[test]
    fn meters_scene_loops_the_whole_track_hot_enough_to_clip() {
        let plan = deck_plan(DemoScene::Meters, LEN, RATE);
        assert_eq!(plan.looping, Some(Loop { start: 0, end: LEN }));
        // Demo tracks peak at 0.25; the clip level is 0.99.
        assert!(0.25 * plan.trim.expect("a trim") >= 0.99);
        assert!(plan.play);
    }

    #[test]
    fn platter_scene_plays_on_from_inside_the_track() {
        let plan = deck_plan(DemoScene::Platter, 64 * 44_100, RATE);
        assert_eq!(plan.seek, Some(8 * 44_100));
        assert!(plan.play && plan.looping.is_none() && plan.trim.is_none());
    }

    #[test]
    fn scenes_that_stage_only_the_window_leave_the_deck_alone() {
        for scene in [DemoScene::Sampler, DemoScene::Stems] {
            assert_eq!(deck_plan(scene, LEN, RATE), DeckPlan::default());
        }
        let track = deck_plan(DemoScene::Track, LEN, RATE);
        assert_eq!(track.seek, Some(LEN / 4));
        assert!(track.looping.is_none() && track.play);
    }
}
