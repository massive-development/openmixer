//! [`TransitionPlanner`]: turns two [`DeckInfo`]s into a [`TransitionPlan`].
//!
//! What it decides, in order:
//! 1. **Feasibility.** Both tracks need a grid and a tempo. A beat-matched style needs the tempo
//!    gap within `max_bpm_ratio`; with `Auto` a wider gap becomes a `Cut`, with an explicit style it
//!    is a [`PlanError::BpmRatio`].
//! 2. **Style** (`Auto` only). Keys that agree on the Camelot wheel give `BassSwap` when both decks
//!    play stems and `Crossfade` otherwise; keys that clash give `EchoOut`, which hides the clash;
//!    an unknown key is treated as neutral (`Crossfade`). `BassSwap` and `StemBridge` fall back to
//!    `Crossfade` unless both decks have stems. `EchoOut` falls back to `Crossfade` when the
//!    snapshot lists the outgoing rack and no echo unit on it is off.
//! 3. **Length.** 8, 16 or 32 beats. Long (32) needs a tight tempo, no key clash and an outro or
//!    intro that long; a key clash or a wide tempo gap gets the short one (8). Shrunk to fit.
//! 4. **Start.** The outgoing mix-out point (else its last outro or breakdown, else late in the
//!    track), rounded to a bar and never earlier than the lead after the current position.
//!    The incoming deck is cued at its mix-in point (else its first bar).
//!
//! The ramps and EQ/FX/stem moves per style are built in `moves.rs`.

use crate::moves::{build_moves, pick_echo_unit};
use crate::plan::{PlanError, PlanOptions, Style, TempoPlan, TransitionPlan, LENGTHS};
use crate::types::{DeckInfo, SectionKind};

/// Anything that can plan a transition.
pub trait TransitionPlanner {
    fn plan(
        &self,
        out: &DeckInfo,
        inc: &DeckInfo,
        style: Style,
        opts: &PlanOptions<'_>,
    ) -> Result<TransitionPlan, PlanError>;
}

/// The planner the app uses. Stateless and deterministic: the same inputs give the same plan.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultPlanner;

/// Rounds `beat` up to a multiple of `bar`, tolerating float noise.
fn ceil_to(beat: f64, bar: f64) -> f64 {
    ((beat / bar) - 1e-9).ceil() * bar
}

fn floor_to(beat: f64, bar: f64) -> f64 {
    ((beat / bar) + 1e-9).floor() * bar
}

fn round_to(beat: f64, bar: f64) -> f64 {
    (beat / bar).round() * bar
}

impl TransitionPlanner for DefaultPlanner {
    fn plan(
        &self,
        out: &DeckInfo,
        inc: &DeckInfo,
        style: Style,
        opts: &PlanOptions<'_>,
    ) -> Result<TransitionPlan, PlanError> {
        if out.deck == inc.deck {
            return Err(PlanError::SameDeck);
        }
        out.grid.as_ref().ok_or(PlanError::NoGrid(out.deck))?;
        let inc_grid = inc.grid.as_ref().ok_or(PlanError::NoGrid(inc.deck))?;
        for (deck, bpm) in [(out.deck, out.bpm), (inc.deck, inc.bpm)] {
            if !(bpm.is_finite() && bpm > 0.0) {
                return Err(PlanError::InvalidBpm(deck));
            }
        }
        let out_rate = if opts.out_rate.is_finite() && opts.out_rate > 0.0 {
            opts.out_rate
        } else {
            1.0
        };
        let follower_rate = out.bpm * out_rate / inc.bpm;
        let distance = (follower_rate - 1.0).abs();
        let tempo_ok = distance <= opts.max_bpm_ratio;

        let key_ok = match (out.key(), inc.key()) {
            (Some(a), Some(b)) => Some(a.compatible(b)),
            _ => None,
        };
        let both_stems = out.has_stems && inc.has_stems;

        let mut resolved = match style {
            Style::Auto if !tempo_ok => Style::Cut,
            Style::Auto => match key_ok {
                Some(false) => Style::EchoOut,
                Some(true) if both_stems => Style::BassSwap,
                _ => Style::Crossfade,
            },
            other => other,
        };
        if resolved != Style::Cut && !tempo_ok {
            return Err(PlanError::BpmRatio {
                distance: distance * 100.0,
                limit: opts.max_bpm_ratio * 100.0,
            });
        }
        if matches!(resolved, Style::BassSwap | Style::StemBridge) && !both_stems {
            resolved = Style::Crossfade;
        }
        // No free echo unit on the outgoing rack: plain crossfade (a tempo gap already chose Cut
        // above, and a Cut needs no unit).
        if resolved == Style::EchoOut && pick_echo_unit(opts, out.deck).is_none() {
            resolved = Style::Crossfade;
        }

        let bar = out.beats_per_bar();
        let remaining = out.length_beats - opts.out_position_beat;
        let min_len = opts.min_length_beats.max(0.0);
        let cut = resolved == Style::Cut;
        if !cut && remaining < min_len {
            return Err(PlanError::OutgoingTooShort {
                remaining: remaining.max(0.0),
                min: min_len,
            });
        }

        let earliest = ceil_to(opts.out_position_beat + opts.lead_beats.max(0.0), bar);
        let length = if cut {
            0.0
        } else {
            pick_length(out, inc, opts, key_ok, distance)
        };
        let start = pick_start(out, opts, length, earliest, bar)?;
        // Shrink the length until it fits before the end of the outgoing track.
        let length = if cut {
            0.0
        } else {
            fit_length(length, out.length_beats - start, min_len).ok_or(PlanError::NoRoom)?
        };
        if cut && start > out.length_beats {
            return Err(PlanError::NoRoom);
        }

        let in_cue_beat = inc.mix_in.unwrap_or_else(|| {
            ceil_to(inc_grid.beat_at_frame(0), inc.beats_per_bar().max(1.0)).max(0.0)
        });
        let inc_remaining = inc.length_beats - in_cue_beat;
        if inc_remaining < length.max(1.0) {
            return Err(PlanError::IncomingTooShort {
                remaining: inc_remaining.max(0.0),
                needed: length.max(1.0),
            });
        }

        let moves = build_moves(resolved, out, inc, start, length, opts);
        Ok(TransitionPlan {
            style: resolved,
            out_deck: out.deck,
            in_deck: inc.deck,
            start_beat: start,
            length_beats: length,
            in_cue_beat,
            steps: moves.steps,
            stem_plan: moves.stem_plan,
            tempo_plan: TempoPlan {
                leader_deck: out.deck,
                follower_deck: inc.deck,
                follower_rate,
                bpm_distance: distance,
            },
            cleanup: moves.cleanup,
            restore_on_cancel: moves.restore,
            route_restore: moves.route_restore,
        })
    }
}

/// The length the planner would like, before it is fitted to the track.
fn pick_length(
    out: &DeckInfo,
    inc: &DeckInfo,
    opts: &PlanOptions<'_>,
    key_ok: Option<bool>,
    distance: f64,
) -> f64 {
    if let Some(want) = opts.length_beats {
        return snap_length(want);
    }
    if key_ok == Some(false) || distance > 0.04 {
        return LENGTHS[0];
    }
    let long = LENGTHS[2];
    let outro = out
        .last_section(SectionKind::Outro)
        .map_or(0.0, |s| s.length_beats());
    let intro = inc
        .section_after(SectionKind::Intro, f64::NEG_INFINITY)
        .map_or(0.0, |s| s.length_beats());
    if distance <= 0.03 && (outro >= long || intro >= long) {
        long
    } else {
        LENGTHS[1]
    }
}

/// The allowed length nearest to `want`.
fn snap_length(want: f64) -> f64 {
    LENGTHS
        .into_iter()
        .min_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()))
        .unwrap_or(LENGTHS[1])
}

/// The longest allowed length not above `length` that fits in `available` beats and is at least
/// `min_len`. `None` when not even the shortest fits.
fn fit_length(length: f64, available: f64, min_len: f64) -> Option<f64> {
    LENGTHS
        .into_iter()
        .rev()
        .filter(|l| *l <= length + 1e-9 && *l + 1e-9 >= min_len)
        .find(|l| *l <= available + 1e-9)
}

/// The bar-aligned start beat on the outgoing grid.
fn pick_start(
    out: &DeckInfo,
    opts: &PlanOptions<'_>,
    length: f64,
    earliest: f64,
    bar: f64,
) -> Result<f64, PlanError> {
    if opts.start_asap {
        return Ok(earliest);
    }
    let preferred = out
        .mix_out
        .or_else(|| out.last_section(SectionKind::Outro).map(|s| s.start_beat))
        .or_else(|| {
            out.sections
                .iter()
                .filter(|s| s.kind == SectionKind::Breakdown && s.start_beat >= earliest)
                .map(|s| s.start_beat)
                .reduce(f64::max)
        })
        .unwrap_or(out.length_beats - length - 2.0 * bar);
    let mut start = round_to(preferred, bar).max(earliest);
    // Too late to fit the length before the end: move earlier, down to the earliest start.
    let latest = floor_to(out.length_beats - length, bar);
    if start > latest {
        start = latest.max(earliest);
    }
    if start > out.length_beats {
        return Err(PlanError::NoRoom);
    }
    Ok(start)
}

#[cfg(test)]
mod tests;
