//! The moves each style makes: crossfader ramp, incoming start, EQ swaps, echo and filter sweeps,
//! stem swaps. Called by the planner once the style, length and start are fixed.
//!
//! Ids used (all from the `om-core` id table): `deck.<d>.play | sync | sync.master`,
//! `mixer.crossfader`, `mixer.ch<N>.eq.low | filter`, `fx.ch<N>.fx<U>.enable | mix` and
//! `stem.<d>.<s>.volume | mute`. Channel `N` is the deck index (the engine maps deck n to channel n).
//!
//! The crossfader ramps linearly in position. With the mixer's default constant-power law that is
//! an equal-power fade: out gain `cos(pi/2 x)`, in gain `sin(pi/2 x)`, squares summing to 1.

use crate::plan::{PlanOptions, Step, Style, EQ_KILL_DB};
use crate::readback::read_param;
use crate::stem_fade::{
    bass_swap, drums_only_bridge, drums_only_bridge_from_snapshot, Curve, StemFade, StemFadePlan,
    BASS,
};
use crate::types::DeckInfo;
use om_core::param_id::{ChannelParam, DeckParam, EqBand, FxParam, FxRack, MixerParam, StemParam};
use om_core::ParamId;

pub(crate) struct Moves {
    pub steps: Vec<Step>,
    pub stem_plan: Option<StemFadePlan>,
    pub cleanup: Vec<(ParamId, f64)>,
    pub restore: Vec<(ParamId, f64)>,
}

fn deck_id(deck: u8, p: DeckParam) -> ParamId {
    ParamId::Deck { deck, p }
}

fn chan_id(ch: u8, p: ChannelParam) -> ParamId {
    ParamId::Mixer(MixerParam::Channel { ch, p })
}

fn xfader_id() -> ParamId {
    ParamId::Mixer(MixerParam::Crossfader)
}

pub(crate) fn build_moves(
    style: Style,
    out: &DeckInfo,
    inc: &DeckInfo,
    start: f64,
    length: f64,
    opts: &PlanOptions<'_>,
) -> Moves {
    let get = |id: &ParamId, default: f64| {
        opts.snapshot
            .and_then(|s| read_param(s, id))
            .unwrap_or(default)
    };
    let (o, i) = (out.deck, inc.deck);
    let mut steps = Vec::new();
    let mut cleanup = vec![
        (deck_id(o, DeckParam::Play), 0.0),
        (deck_id(i, DeckParam::SyncMaster), 1.0),
        (deck_id(o, DeckParam::Sync), 0.0),
    ];
    let mut restore: Vec<(ParamId, f64)> = Vec::new();
    let mut stem_plan = None;

    // The incoming deck follows the outgoing deck's beat and starts on the bar.
    steps.push(Step::set(
        start,
        deck_id(o, DeckParam::SyncMaster),
        0.0,
        1.0,
    ));
    steps.push(Step::set(start, deck_id(i, DeckParam::Sync), 0.0, 1.0));
    steps.push(Step::set(start, deck_id(i, DeckParam::Play), 0.0, 1.0));

    let (side_out, side_in) = if opts.out_on_left {
        (0.0, 1.0)
    } else {
        (1.0, 0.0)
    };
    let xf_from = get(&xfader_id(), side_out).clamp(0.0, 1.0);
    let mid = start + length / 2.0;

    match style {
        Style::Cut => {
            steps.push(Step::set(start, xfader_id(), xf_from, side_in));
            steps.push(Step::set(start, deck_id(o, DeckParam::Play), 1.0, 0.0));
        }
        Style::Crossfade | Style::Auto => {
            steps.push(Step::new(
                start,
                length,
                xfader_id(),
                xf_from,
                side_in,
                Curve::Linear,
            ));
            let low = EqBand::Low;
            let in_low = chan_id(i, ChannelParam::Eq(low));
            let out_low = chan_id(o, ChannelParam::Eq(low));
            let in_orig = get(&in_low, 0.0);
            let out_orig = get(&out_low, 0.0);
            // Bass swap on the EQs: the incoming bass is out of the way until the midpoint.
            steps.push(Step::set(start, in_low.clone(), in_orig, EQ_KILL_DB));
            steps.push(Step::new(
                mid,
                1.0,
                out_low.clone(),
                out_orig,
                EQ_KILL_DB,
                Curve::Linear,
            ));
            steps.push(Step::set(mid, in_low.clone(), EQ_KILL_DB, in_orig));
            restore.push((in_low, in_orig));
            restore.push((out_low.clone(), out_orig));
            cleanup.push((out_low, out_orig));
        }
        Style::EchoOut => {
            let rack = FxRack::Channel(o);
            let fx = |p| ParamId::Fx {
                rack,
                unit: Some(opts.echo_unit),
                p,
            };
            let enable = fx(FxParam::Enable);
            steps.push(Step::set(start, enable.clone(), 0.0, 1.0));
            steps.push(Step::set(start, fx(FxParam::Mix), 0.0, 0.5));
            let (ramp_at, ramp_len) = (start + length / 4.0, length / 2.0);
            steps.push(Step::new(
                ramp_at,
                ramp_len,
                xfader_id(),
                xf_from,
                side_in,
                Curve::Linear,
            ));
            let filter = chan_id(o, ChannelParam::Filter);
            let filter_orig = get(&filter, 0.0);
            steps.push(Step::new(
                ramp_at,
                ramp_len,
                filter.clone(),
                filter_orig,
                0.6,
                Curve::Linear,
            ));
            restore.push((enable.clone(), 0.0));
            restore.push((filter.clone(), filter_orig));
            cleanup.push((enable, 0.0));
            cleanup.push((filter, filter_orig));
        }
        Style::BassSwap | Style::StemBridge => {
            steps.push(Step::new(
                start,
                length,
                xfader_id(),
                xf_from,
                side_in,
                Curve::Linear,
            ));
            let plan = stem_moves(style, out, inc, start, length, opts);
            // Put the outgoing deck's stems back once it has stopped, so the next track loaded on
            // it does not start with a silent bass or muted stems.
            cleanup.extend(
                plan.restore_values()
                    .into_iter()
                    .filter(|(id, _)| matches!(id, ParamId::Stem { deck, .. } if *deck == o)),
            );
            stem_plan = Some(plan);
        }
    }

    steps.sort_by(|a, b| a.at_beat.total_cmp(&b.at_beat));
    Moves {
        steps,
        stem_plan,
        cleanup,
        restore,
    }
}

/// The stem fades of `BassSwap` and `StemBridge`. Start gains and mutes come from the snapshot
/// when there is one (a cancel restores what the user had).
fn stem_moves(
    style: Style,
    out: &DeckInfo,
    inc: &DeckInfo,
    start: f64,
    length: f64,
    opts: &PlanOptions<'_>,
) -> StemFadePlan {
    let (o, i) = (out.deck, inc.deck);
    let gain = |deck: u8| -> f64 {
        opts.snapshot
            .and_then(|s| {
                read_param(
                    s,
                    &ParamId::Stem {
                        deck,
                        stem: BASS,
                        p: StemParam::Volume,
                    },
                )
            })
            .unwrap_or(1.0)
    };
    // The incoming bass is silent from the start, then swaps in over two beats at the midpoint.
    let premute = StemFade {
        stem: BASS,
        deck: i,
        param: StemParam::Volume,
        from: gain(i),
        to: 0.0,
        start_beat: start,
        length_beats: 0.0,
        curve: Curve::Linear,
    };
    let swap_len = 2.0_f64.min(length);
    let swap_at = start + length / 2.0 - swap_len / 2.0;
    let swap = bass_swap(gain(o), 0.0, o, i, swap_at, swap_len);
    let mut fades = vec![premute];
    fades.extend_from_slice(swap.fades());
    if style == Style::StemBridge {
        let bridge_at = start + length / 2.0;
        let bridge = match opts.snapshot {
            Some(snap) => drums_only_bridge_from_snapshot(snap, o, bridge_at, length / 2.0),
            None => drums_only_bridge([false; 4], o, bridge_at, length / 2.0),
        };
        fades.extend_from_slice(bridge.fades());
    }
    StemFadePlan::new(fades)
}
