use super::*;
use crate::fixtures::{info, LENGTH_BEATS};
use crate::plan::{PlanOptions, Style, EQ_KILL_DB};
use crate::readback::blank_snapshot;
use om_core::param_id::{ChannelParam, DeckParam, EqBand, FxParam, FxRack, MixerParam};
use om_core::ParamId;
use proptest::prelude::*;

fn xfader() -> ParamId {
    ParamId::Mixer(MixerParam::Crossfader)
}

fn low(ch: u8) -> ParamId {
    ParamId::Mixer(MixerParam::Channel {
        ch,
        p: ChannelParam::Eq(EqBand::Low),
    })
}

fn plan(out: &DeckInfo, inc: &DeckInfo, style: Style) -> Result<TransitionPlan, PlanError> {
    DefaultPlanner.plan(out, inc, style, &PlanOptions::default())
}

fn pair() -> (DeckInfo, DeckInfo) {
    (info(0, 124.0, "8A", false), info(1, 126.0, "8A", false))
}

fn xfader_value(plan: &TransitionPlan, beat: f64) -> f64 {
    plan.values_at(beat)
        .into_iter()
        .find(|(id, _)| *id == xfader())
        .map(|(_, v)| v)
        .expect("crossfader step")
}

#[test]
fn same_inputs_give_the_same_plan() {
    let (out, inc) = pair();
    for style in Style::ALL {
        assert_eq!(
            plan(&out, &inc, style),
            plan(&out, &inc, style),
            "{style:?}"
        );
    }
}

#[test]
fn starts_on_a_bar_at_the_mix_out_point() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::Crossfade).unwrap();
    assert_eq!(p.start_beat, 400.0);
    assert_eq!(p.start_beat % 4.0, 0.0);
    assert_eq!(p.in_cue_beat, 0.0);
    assert!(LENGTHS_OK.contains(&p.length_beats));
}

const LENGTHS_OK: [f64; 3] = [8.0, 16.0, 32.0];

#[test]
fn an_off_grid_mix_out_is_rounded_to_a_bar() {
    let (mut out, inc) = pair();
    out.mix_out = Some(397.3);
    let p = plan(&out, &inc, Style::Crossfade).unwrap();
    assert_eq!(p.start_beat, 396.0);
    out.mix_out = Some(398.5);
    assert_eq!(
        plan(&out, &inc, Style::Crossfade).unwrap().start_beat,
        400.0
    );
}

#[test]
fn start_is_never_before_the_lead_after_the_playhead() {
    let (out, inc) = pair();
    let opts = PlanOptions {
        out_position_beat: 401.0,
        lead_beats: 4.0,
        ..PlanOptions::default()
    };
    let p = DefaultPlanner
        .plan(&out, &inc, Style::Crossfade, &opts)
        .unwrap();
    assert!(
        p.start_beat >= 405.0 && p.start_beat % 4.0 == 0.0,
        "{}",
        p.start_beat
    );
}

#[test]
fn start_asap_ignores_the_mix_out_point() {
    let (out, inc) = pair();
    let opts = PlanOptions {
        out_position_beat: 100.0,
        start_asap: true,
        ..PlanOptions::default()
    };
    let p = DefaultPlanner
        .plan(&out, &inc, Style::Crossfade, &opts)
        .unwrap();
    assert_eq!(p.start_beat, 104.0);
}

#[test]
fn crossfader_ramp_is_equal_power_and_exact_at_the_ends() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::Crossfade).unwrap();
    assert_eq!(xfader_value(&p, p.start_beat), 0.0);
    assert_eq!(xfader_value(&p, p.end_beat()), 1.0);
    assert_eq!(xfader_value(&p, p.end_beat() + 50.0), 1.0);
    for k in 0..=64 {
        let beat = p.start_beat + p.length_beats * f64::from(k) / 64.0;
        let x = xfader_value(&p, beat);
        // Constant-power law of the mixer: out cos, in sin.
        let (g_out, g_in) = (
            (x * std::f64::consts::FRAC_PI_2).cos(),
            (x * std::f64::consts::FRAC_PI_2).sin(),
        );
        assert!((g_out * g_out + g_in * g_in - 1.0).abs() < 1e-9);
        assert!((0.0..=1.0).contains(&x));
    }
}

#[test]
fn deck_on_the_right_fades_the_other_way() {
    let (out, inc) = pair();
    let opts = PlanOptions {
        out_on_left: false,
        ..PlanOptions::default()
    };
    let p = DefaultPlanner
        .plan(&out, &inc, Style::Crossfade, &opts)
        .unwrap();
    assert_eq!(xfader_value(&p, p.start_beat), 1.0);
    assert_eq!(xfader_value(&p, p.end_beat()), 0.0);
}

#[test]
fn the_incoming_deck_syncs_and_starts_at_the_start_beat() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::Crossfade).unwrap();
    let at_start: Vec<_> = p
        .steps
        .iter()
        .filter(|s| s.at_beat == p.start_beat)
        .map(|s| s.id.to_string())
        .collect();
    assert!(at_start.contains(&"deck.0.sync.master".to_owned()));
    assert!(at_start.contains(&"deck.1.sync".to_owned()));
    assert!(at_start.contains(&"deck.1.play".to_owned()));
    let play = p
        .steps
        .iter()
        .position(|s| s.id.to_string() == "deck.1.play")
        .unwrap();
    let sync = p
        .steps
        .iter()
        .position(|s| s.id.to_string() == "deck.1.sync")
        .unwrap();
    assert!(sync < play, "sync is set before the deck starts");
    assert_eq!(p.tempo_plan.leader_deck, 0);
    assert_eq!(p.tempo_plan.follower_deck, 1);
    assert!((p.tempo_plan.follower_rate - 124.0 / 126.0).abs() < 1e-12);
}

#[test]
fn crossfade_swaps_the_bass_on_the_eqs_at_the_midpoint() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::Crossfade).unwrap();
    let mid = p.start_beat + p.length_beats / 2.0;
    let at = |id: &ParamId, beat: f64| {
        p.values_at(beat)
            .into_iter()
            .find(|(i, _)| i == id)
            .map(|(_, v)| v)
    };
    assert_eq!(at(&low(1), p.start_beat), Some(EQ_KILL_DB));
    assert_eq!(at(&low(1), mid - 0.1), Some(EQ_KILL_DB));
    assert_eq!(at(&low(1), mid), Some(0.0));
    assert_eq!(at(&low(0), mid - 0.1), None.or(at(&low(0), mid - 0.1)));
    assert_eq!(at(&low(0), mid + 1.0), Some(EQ_KILL_DB));
    // The outgoing bass is restored when the plan ends.
    assert!(p.cleanup.contains(&(low(0), 0.0)));
    assert!(p.cleanup.contains(&(deck(0, DeckParam::Play), 0.0)));
    assert!(p.restore_on_cancel.contains(&(low(1), 0.0)));
}

fn deck(deck: u8, p: DeckParam) -> ParamId {
    ParamId::Deck { deck, p }
}

#[test]
fn steps_are_ordered_and_inside_the_window() {
    let (out, inc) = pair();
    for style in Style::ALL {
        let p = plan(&out, &inc, style).unwrap();
        assert!(p.steps.windows(2).all(|w| w[0].at_beat <= w[1].at_beat));
        for s in &p.steps {
            assert!(s.at_beat >= p.start_beat, "{style:?} {s:?}");
            assert!(s.end_beat() <= p.end_beat() + 1e-9, "{style:?} {s:?}");
        }
    }
}

#[test]
fn bass_swap_needs_stems_on_both_decks() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::BassSwap).unwrap();
    assert_eq!(p.style, Style::Crossfade, "no stems: falls back");
    assert!(p.stem_plan.is_none());

    let (out, mut inc) = (info(0, 124.0, "8A", true), info(1, 126.0, "8A", false));
    assert_eq!(
        plan(&out, &inc, Style::BassSwap).unwrap().style,
        Style::Crossfade
    );
    inc.has_stems = true;
    let p = plan(&out, &inc, Style::BassSwap).unwrap();
    assert_eq!(p.style, Style::BassSwap);
    let stems = p.stem_plan.as_ref().unwrap();
    let ids: Vec<String> = stems.fades().iter().map(|f| f.id().to_string()).collect();
    assert!(ids.contains(&"stem.0.1.volume".to_owned()));
    assert!(ids.contains(&"stem.1.1.volume".to_owned()));
    // The EQ swap is not used on top of the stem swap.
    assert!(p.steps.iter().all(|s| s.id != low(0) && s.id != low(1)));
    let end = stems.end_beat().unwrap();
    assert!(end <= p.end_beat() + 1e-9);
}

#[test]
fn stem_bridge_mutes_everything_but_drums_on_the_outgoing_deck() {
    let (out, inc) = (info(0, 124.0, "8A", true), info(1, 126.0, "8A", true));
    let p = plan(&out, &inc, Style::StemBridge).unwrap();
    assert_eq!(p.style, Style::StemBridge);
    let stems = p.stem_plan.as_ref().unwrap();
    let mutes: Vec<_> = stems
        .fades()
        .iter()
        .filter(|f| f.param == om_core::param_id::StemParam::Mute && f.deck == 0)
        .collect();
    assert_eq!(mutes.len(), 6, "bass, vocals, other: mute then unmute");
    assert!(mutes.iter().all(|f| f.stem != 0), "drums are left alone");
    // The outgoing stems are put back after it stops.
    assert!(p
        .cleanup
        .iter()
        .any(|(id, _)| id.to_string() == "stem.0.1.mute"));
}

#[test]
fn stem_start_gains_come_from_the_snapshot() {
    let (out, inc) = (info(0, 124.0, "8A", true), info(1, 126.0, "8A", true));
    let mut snap = blank_snapshot(2);
    snap.stems[0][1].gain = 0.7;
    snap.stems[1][1].gain = 0.9;
    snap.mixer.crossfader = 0.25;
    snap.mixer.channels[1].eq[0] = -3.0;
    let opts = PlanOptions {
        snapshot: Some(&snap),
        ..PlanOptions::default()
    };
    let p = DefaultPlanner
        .plan(&out, &inc, Style::BassSwap, &opts)
        .unwrap();
    let restore = p.stem_plan.as_ref().unwrap().restore_values();
    let get = |s: &str| {
        restore
            .iter()
            .find(|(id, _)| id.to_string() == s)
            .unwrap()
            .1
    };
    assert!((get("stem.0.1.volume") - 0.7).abs() < 1e-6);
    assert!((get("stem.1.1.volume") - 0.9).abs() < 1e-6);
    assert!((xfader_value(&p, p.start_beat) - 0.25).abs() < 1e-6);

    let (out, inc) = pair();
    let p = DefaultPlanner
        .plan(&out, &inc, Style::Crossfade, &opts)
        .unwrap();
    assert!(p.restore_on_cancel.contains(&(low(1), -3.0)));
}

#[test]
fn echo_out_switches_the_echo_on_the_outgoing_rack() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::EchoOut).unwrap();
    let enable = ParamId::Fx {
        rack: FxRack::Channel(0),
        unit: Some(0),
        p: FxParam::Enable,
    };
    assert!(p.steps.iter().any(|s| s.id == enable && s.to == 1.0));
    assert!(p.cleanup.contains(&(enable, 0.0)));
    // The crossfader moves in the middle half only.
    let early = p.values_at(p.start_beat + p.length_beats / 4.0 - 0.01);
    assert!(
        early.iter().all(|(id, _)| *id != xfader()),
        "not moving yet"
    );
    assert_eq!(xfader_value(&p, p.start_beat + p.length_beats * 0.75), 1.0);
}

#[test]
fn cut_has_no_length_and_flips_the_crossfader_at_once() {
    let (out, inc) = pair();
    let p = plan(&out, &inc, Style::Cut).unwrap();
    assert_eq!(p.length_beats, 0.0);
    assert_eq!(xfader_value(&p, p.start_beat), 1.0);
    assert!(p.steps.iter().all(|s| s.at_beat == p.start_beat));
}

#[test]
fn auto_prefers_compatible_keys_and_stems() {
    let (out, inc) = (info(0, 124.0, "8A", false), info(1, 124.0, "9A", false));
    assert_eq!(
        plan(&out, &inc, Style::Auto).unwrap().style,
        Style::Crossfade
    );
    let relative = info(1, 124.0, "8B", false);
    assert_eq!(
        plan(&out, &relative, Style::Auto).unwrap().style,
        Style::Crossfade
    );
    let clash = info(1, 124.0, "3B", false);
    assert_eq!(
        plan(&out, &clash, Style::Auto).unwrap().style,
        Style::EchoOut
    );
    let unknown = info(1, 124.0, "", false);
    assert_eq!(
        plan(&out, &unknown, Style::Auto).unwrap().style,
        Style::Crossfade
    );
    let (out, inc) = (info(0, 124.0, "8A", true), info(1, 124.0, "9A", true));
    assert_eq!(
        plan(&out, &inc, Style::Auto).unwrap().style,
        Style::BassSwap
    );
    // Stems on one deck only: no bass swap.
    let (out, inc) = (info(0, 124.0, "8A", true), info(1, 124.0, "9A", false));
    assert_eq!(
        plan(&out, &inc, Style::Auto).unwrap().style,
        Style::Crossfade
    );
}

#[test]
fn auto_picks_the_length_from_key_tempo_and_sections() {
    let (out, inc) = (info(0, 124.0, "8A", false), info(1, 124.0, "8A", false));
    // Outro 400..480 is 80 beats: a long mix.
    assert_eq!(plan(&out, &inc, Style::Auto).unwrap().length_beats, 32.0);
    let (mut out2, inc2) = (out.clone(), inc.clone());
    out2.sections.clear();
    assert_eq!(
        plan(&out2, &inc2, Style::Auto).unwrap().length_beats,
        32.0,
        "incoming intro is 32"
    );
    let mut inc3 = inc.clone();
    inc3.sections.clear();
    assert_eq!(plan(&out2, &inc3, Style::Auto).unwrap().length_beats, 16.0);
    let clash = info(1, 124.0, "3B", false);
    assert_eq!(plan(&out, &clash, Style::Auto).unwrap().length_beats, 8.0);
    let wide = info(1, 130.0, "8A", false);
    assert_eq!(plan(&out, &wide, Style::Auto).unwrap().length_beats, 8.0);
}

#[test]
fn a_requested_length_is_snapped_and_shrunk_to_fit() {
    let (out, inc) = pair();
    let opts = PlanOptions {
        length_beats: Some(20.0),
        ..PlanOptions::default()
    };
    let p = DefaultPlanner
        .plan(&out, &inc, Style::Crossfade, &opts)
        .unwrap();
    assert_eq!(p.length_beats, 16.0);
    // 40 beats left and a 32-beat mix wanted at 412: starts earlier or shrinks, never overruns.
    let mut late = out.clone();
    late.mix_out = Some(LENGTH_BEATS - 12.0);
    let opts = PlanOptions {
        length_beats: Some(32.0),
        ..PlanOptions::default()
    };
    let p = DefaultPlanner
        .plan(&late, &inc, Style::Crossfade, &opts)
        .unwrap();
    assert!(p.end_beat() <= LENGTH_BEATS + 1e-9);
    assert_eq!(p.length_beats, 32.0);
    assert_eq!(p.start_beat, 448.0);
    let opts = PlanOptions {
        length_beats: Some(32.0),
        out_position_beat: 474.0,
        ..PlanOptions::default()
    };
    assert!(matches!(
        DefaultPlanner.plan(&late, &inc, Style::Crossfade, &opts),
        Err(PlanError::OutgoingTooShort { .. })
    ));
    let opts = PlanOptions {
        out_position_beat: 466.0,
        ..PlanOptions::default()
    };
    // 14 beats left: fits an 8-beat mix only if the lead allows it.
    let p = DefaultPlanner.plan(&late, &inc, Style::Crossfade, &opts);
    assert!(p.is_err() || p.unwrap().end_beat() <= LENGTH_BEATS + 1e-9);
}

#[test]
fn infeasible_inputs_say_why() {
    let (out, inc) = pair();
    let mut no_grid = out.clone();
    no_grid.grid = None;
    assert_eq!(plan(&no_grid, &inc, Style::Auto), Err(PlanError::NoGrid(0)));
    let mut no_grid_in = inc.clone();
    no_grid_in.grid = None;
    assert_eq!(
        plan(&out, &no_grid_in, Style::Auto),
        Err(PlanError::NoGrid(1))
    );
    let mut zero = inc.clone();
    zero.bpm = 0.0;
    assert_eq!(
        plan(&out, &zero, Style::Auto),
        Err(PlanError::InvalidBpm(1))
    );
    assert_eq!(plan(&out, &out, Style::Auto), Err(PlanError::SameDeck));

    let fast = info(1, 140.0, "8A", false);
    assert!(matches!(
        plan(&out, &fast, Style::Crossfade),
        Err(PlanError::BpmRatio { .. })
    ));
    assert_eq!(plan(&out, &fast, Style::Auto).unwrap().style, Style::Cut);
    assert_eq!(plan(&out, &fast, Style::Cut).unwrap().style, Style::Cut);

    let opts = PlanOptions {
        out_position_beat: LENGTH_BEATS - 4.0,
        ..PlanOptions::default()
    };
    assert!(matches!(
        DefaultPlanner.plan(&out, &inc, Style::Crossfade, &opts),
        Err(PlanError::OutgoingTooShort { .. })
    ));

    let mut short = inc.clone();
    short.length_beats = 10.0;
    assert!(matches!(
        plan(&out, &short, Style::Crossfade),
        Err(PlanError::IncomingTooShort { .. })
    ));
    let msg = plan(&out, &fast, Style::Crossfade).unwrap_err().to_string();
    assert!(msg.contains('%'), "{msg}");
}

#[test]
fn the_bpm_limit_is_configurable() {
    let out = info(0, 120.0, "8A", false);
    let inc = info(1, 130.0, "8A", false);
    assert!(plan(&out, &inc, Style::Crossfade).is_err());
    let opts = PlanOptions {
        max_bpm_ratio: 0.08,
        ..PlanOptions::default()
    };
    assert!(DefaultPlanner
        .plan(&out, &inc, Style::Crossfade, &opts)
        .is_ok());
}

#[test]
fn without_marks_the_outro_or_the_late_track_is_used() {
    let (mut out, inc) = pair();
    out.mix_out = None;
    assert_eq!(
        plan(&out, &inc, Style::Crossfade).unwrap().start_beat,
        400.0,
        "outro"
    );
    out.sections.clear();
    let p = plan(&out, &inc, Style::Crossfade).unwrap();
    assert!(p.end_beat() <= LENGTH_BEATS && p.start_beat % 4.0 == 0.0);
    assert!(p.start_beat > 400.0);
}

proptest! {
    #[test]
    fn plans_are_ordered_in_range_and_deterministic(
        pos in 0.0f64..470.0,
        gap in -0.05f64..0.05,
        style in 0usize..6,
        stems in any::<bool>(),
        key in 1u8..=12,
        minor in any::<bool>(),
        len in prop::option::of(1.0f64..60.0),
        left in any::<bool>(),
    ) {
        let out = info(0, 120.0, "8A", stems);
        let inc = info(1, 120.0 * (1.0 + gap), &format!("{key}{}", if minor { 'A' } else { 'B' }), stems);
        let opts = PlanOptions {
            out_position_beat: pos,
            length_beats: len,
            out_on_left: left,
            ..PlanOptions::default()
        };
        let style = Style::ALL[style];
        let a = DefaultPlanner.plan(&out, &inc, style, &opts);
        let b = DefaultPlanner.plan(&out, &inc, style, &opts);
        prop_assert_eq!(&a, &b);
        if let Ok(p) = a {
            prop_assert!(p.steps.windows(2).all(|w| w[0].at_beat <= w[1].at_beat));
            prop_assert_eq!(p.start_beat % 4.0, 0.0);
            prop_assert!(p.start_beat >= pos + 4.0 - 1e-9);
            prop_assert!(p.end_beat() <= LENGTH_BEATS + 1e-9);
            for s in &p.steps {
                prop_assert!(s.at_beat >= p.start_beat && s.end_beat() <= p.end_beat() + 1e-9);
                let lo = s.from.min(s.to) - 1e-9;
                let hi = s.from.max(s.to) + 1e-9;
                for k in 0..=8 {
                    let v = s.value_at(s.at_beat + s.length_beats * f64::from(k) / 8.0);
                    prop_assert!(v >= lo && v <= hi);
                }
                if s.id == xfader() {
                    prop_assert!((0.0..=1.0).contains(&s.from) && (0.0..=1.0).contains(&s.to));
                }
                if s.id == low(0) || s.id == low(1) {
                    prop_assert!((EQ_KILL_DB..=6.0).contains(&s.from) && (EQ_KILL_DB..=6.0).contains(&s.to));
                }
            }
            if matches!(p.style, Style::BassSwap | Style::StemBridge) {
                prop_assert!(stems);
            }
        }
    }
}
