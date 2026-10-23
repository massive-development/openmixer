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

/// Name of the effect an echo-out needs, as the engine's FX units report it.
pub(crate) const ECHO_EFFECT: &str = "echo";

pub(crate) struct Moves {
    pub steps: Vec<Step>,
    pub stem_plan: Option<StemFadePlan>,
    pub cleanup: Vec<(ParamId, f64)>,
    pub restore: Vec<(ParamId, f64)>,
    pub route_restore: Vec<(ParamId, f64)>,
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

fn assign_id(ch: u8) -> ParamId {
    chan_id(ch, ChannelParam::Xfader)
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
    // Crossfader assignments the route changes take effect at the start and go back at the end.
    let mut route_restore: Vec<(ParamId, f64)> = Vec::new();
    for &(deck, was, to) in &opts.route.reassign {
        steps.push(Step::set(start, assign_id(deck), was, to));
        route_restore.push((assign_id(deck), was));
    }
    cleanup.extend(route_restore.iter().cloned());

    // The incoming deck follows the outgoing deck's beat and starts on the bar.
    steps.push(Step::set(
        start,
        deck_id(o, DeckParam::SyncMaster),
        0.0,
        1.0,
    ));
    steps.push(Step::set(start, deck_id(i, DeckParam::Sync), 0.0, 1.0));
    steps.push(Step::set(start, deck_id(i, DeckParam::Play), 0.0, 1.0));

    let (side_out, side_in) = if opts.route.out_left {
        (0.0, 1.0)
    } else {
        (1.0, 0.0)
    };
    // A reassigned channel is at its full level only at its side's edge, so with the crossfader
    // anywhere else (a centred one, say) its level would dip the moment the assignment changes. The
    // ramp therefore starts from the outgoing side's edge whenever the routing changes.
    let xf_now = get(&xfader_id(), side_out).clamp(0.0, 1.0);
    let xf_from = if opts.route.reassign.is_empty() {
        xf_now
    } else {
        side_out
    };
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
            // The planner only picks EchoOut when a unit is free; the configured one is the
            // last resort for a caller that built the moves by hand.
            let unit = pick_echo_unit(opts, o).unwrap_or(opts.echo_unit);
            let (was_on, was_mix) = echo_unit_state(opts, o, unit);
            let fx = |p| ParamId::Fx {
                rack,
                unit: Some(unit),
                p,
            };
            let enable = fx(FxParam::Enable);
            steps.push(Step::set(start, enable.clone(), was_on, 1.0));
            steps.push(Step::set(start, fx(FxParam::Mix), was_mix, 0.5));
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
            // The unit goes back to the switch and send it had before the transition: a unit left
            // at the echo's send level would keep ringing on the next track that uses it.
            let mix = fx(FxParam::Mix);
            restore.push((enable.clone(), was_on));
            restore.push((mix.clone(), was_mix));
            restore.push((filter.clone(), filter_orig));
            cleanup.push((enable, was_on));
            cleanup.push((mix, was_mix));
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
        route_restore,
    }
}

/// The echo unit to use on channel `ch`'s rack: the configured one when it runs the echo and is
/// off, else the first other echo unit that is off. `None` when the snapshot lists the rack and
/// none is free. Without a snapshot, or with one that does not list the rack, the configured unit
/// (nothing can be checked).
pub(crate) fn pick_echo_unit(opts: &PlanOptions<'_>, ch: u8) -> Option<u16> {
    let Some(units) = opts
        .snapshot
        .and_then(|s| s.fx.channels.get(usize::from(ch)))
    else {
        return Some(opts.echo_unit);
    };
    let free = |i: usize| {
        units
            .get(i)
            .is_some_and(|u| u.effect == ECHO_EFFECT && !u.enabled)
    };
    let configured = usize::from(opts.echo_unit);
    let pick = if free(configured) {
        Some(configured)
    } else {
        (0..units.len()).find(|&i| free(i))
    };
    pick.and_then(|i| u16::try_from(i).ok())
}

/// The switch and mix (as control values) the unit has now; off and zero when unknown.
fn echo_unit_state(opts: &PlanOptions<'_>, ch: u8, unit: u16) -> (f64, f64) {
    opts.snapshot
        .and_then(|s| s.fx.unit(usize::from(ch), usize::from(unit)))
        .map_or((0.0, 0.0), |u| {
            (if u.enabled { 1.0 } else { 0.0 }, f64::from(u.mix))
        })
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
