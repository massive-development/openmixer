//! Stem ramps for transitions (Todo 39, Wave 7 S8).
//!
//! A [`StemFade`] moves one stem parameter of one deck from `from` to `to` between `start_beat` and
//! `start_beat + length_beats`. A [`StemFadePlan`] is a set of fades. Values come from beats, so a
//! plan is a pure function of the beat position: asking twice gives the same values, and a fade
//! always ends on exactly `to`.
//!
//! [`StemFadeRunner`] applies a plan on the control thread. Each control tick it sends the
//! `Command::Param` values for the fades that overlap the beats since the last tick. A fade whose
//! deck has no stems is skipped and reported, once. The runner never blocks and never touches the
//! audio thread; it only calls [`CommandSink::send`].
//!
//! Stem indices follow `StemKind::ALL` (drums, bass, vocals, other), the order of the stem files in
//! the stem cache and of `stem.<d>.<s>` ids.
//!
//! Threads: control thread only.

use om_core::param_id::StemParam;
use om_core::{Command, CommandError, CommandSink, DeckSnapshot, EngineSnapshot, ParamId};
use std::ops::Range;

/// Stem index of drums in `stem.<d>.<s>` ids.
pub const DRUMS: u8 = 0;
/// Stem index of bass.
pub const BASS: u8 = 1;
/// Stem index of vocals.
pub const VOCALS: u8 = 2;
/// Stem index of the other stem (everything that is not drums, bass or vocals).
pub const OTHER: u8 = 3;

/// The shape of a ramp between two values.
///
/// - `Linear`: straight line in value.
/// - `EqualPower`: straight line in power (`value^2`) when both ends are non-negative, so a fade out
///   and the matching fade in from the same `t` sum to 1 in power. Otherwise linear.
/// - `Exponential`: geometric (`from * (to / from)^t`) when both ends are positive, which is even in
///   dB. When an end is zero or negative it falls back to a quadratic ease-in (`from + (to - from) * t^2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    EqualPower,
    Exponential,
}

impl Curve {
    /// The value at `t` (clamped to 0 to 1) on the ramp from `from` to `to`. Both endpoints are
    /// exact, and the result stays within `[min(from, to), max(from, to)]`.
    pub fn value(self, from: f64, to: f64, t: f64) -> f64 {
        if t.is_nan() || t <= 0.0 {
            return from;
        }
        if t >= 1.0 {
            return to;
        }
        let v = match self {
            Self::Linear => from + (to - from) * t,
            Self::EqualPower if from >= 0.0 && to >= 0.0 => {
                (from * from + (to * to - from * from) * t).sqrt()
            }
            Self::EqualPower => from + (to - from) * t,
            Self::Exponential if from > 0.0 && to > 0.0 => from * (to / from).powf(t),
            Self::Exponential => from + (to - from) * t * t,
        };
        v.clamp(from.min(to), from.max(to))
    }
}

/// One ramp on one stem parameter. `param` is `Volume` (gain 0 to 1.5), `Filter` (-1 to +1) or
/// `Mute`. A `Mute` fade is a step: it reads `to` from `start_beat` on, whatever its length and
/// curve, because the engine reads mute as on at 0.5 or above. A fade with `length_beats <= 0`
/// is a step too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemFade {
    pub stem: u8,
    pub deck: u8,
    pub param: StemParam,
    pub from: f64,
    pub to: f64,
    pub start_beat: f64,
    pub length_beats: f64,
    pub curve: Curve,
}

impl StemFade {
    /// The beat at which the ramp reaches `to`.
    pub fn end_beat(&self) -> f64 {
        self.start_beat + self.length_beats.max(0.0)
    }

    /// The `stem.<deck>.<stem>.<param>` id this fade drives. Not validated here; see
    /// [`ParamId::validate`] (the runner checks it).
    pub fn id(&self) -> ParamId {
        ParamId::Stem {
            deck: self.deck,
            stem: self.stem,
            p: self.param,
        }
    }

    /// The value at `beat`: `from` up to the start, `to` from the end on, and the curve between.
    /// A NaN beat reads `from`.
    pub fn value_at(&self, beat: f64) -> f64 {
        if beat.is_nan() {
            return self.from;
        }
        if self.param == StemParam::Mute || self.length_beats <= 0.0 {
            return if beat >= self.start_beat {
                self.to
            } else {
                self.from
            };
        }
        if beat <= self.start_beat {
            return self.from;
        }
        if beat >= self.end_beat() {
            return self.to;
        }
        let t = (beat - self.start_beat) / self.length_beats;
        self.curve.value(self.from, self.to, t)
    }

    /// True when the fade's window `[start, end]` overlaps `range` (both ends inclusive).
    fn overlaps(&self, range: &Range<f64>) -> bool {
        self.start_beat <= range.end && self.end_beat() >= range.start
    }
}

/// A set of fades, applied together.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StemFadePlan {
    fades: Vec<StemFade>,
}

impl StemFadePlan {
    pub fn new(fades: Vec<StemFade>) -> Self {
        Self { fades }
    }

    pub fn fades(&self) -> &[StemFade] {
        &self.fades
    }

    /// The beat at which the last fade reaches its end, or `None` for an empty plan.
    pub fn end_beat(&self) -> Option<f64> {
        self.fades
            .iter()
            .map(StemFade::end_beat)
            .fold(None, |acc, e| Some(acc.map_or(e, |a: f64| a.max(e))))
    }

    /// The `Command::Param` values for the fades that overlap `beats`, each at the value it has at
    /// the end of that range. A fade whose window ends before `beats.start` is not included, so a
    /// caller that covers consecutive ranges sends each fade's final value once it has passed.
    /// Pure: the same range gives the same values, so repeating it is harmless.
    pub fn steps_at(&self, beats: Range<f64>) -> Vec<(ParamId, f64)> {
        let now = beats.end;
        self.fades
            .iter()
            .filter(|f| f.overlaps(&beats))
            .map(|f| (f.id(), f.value_at(now)))
            .collect()
    }

    /// The values that put every stem of this plan back to its `from` state: one entry per
    /// parameter, from the first fade that names it. Use it to undo a cancelled plan.
    pub fn restore_values(&self) -> Vec<(ParamId, f64)> {
        first_values(self.fades.iter())
    }
}

/// `(id, from)` once per id, taking the first fade in order that names it.
fn first_values<'a>(fades: impl Iterator<Item = &'a StemFade>) -> Vec<(ParamId, f64)> {
    let mut out: Vec<(ParamId, f64)> = Vec::new();
    for f in fades {
        let id = f.id();
        if !out.iter().any(|(seen, _)| *seen == id) {
            out.push((id, f.from));
        }
    }
    out
}

fn stem_fade(
    deck: u8,
    stem: u8,
    param: StemParam,
    (from, to): (f64, f64),
    (start_beat, length_beats): (f64, f64),
    curve: Curve,
) -> StemFade {
    StemFade {
        stem,
        deck,
        param,
        from,
        to,
        start_beat,
        length_beats,
        curve,
    }
}

/// Bass swap: the outgoing deck's bass fades out while the incoming deck's bass fades in over the
/// same window. Equal power, so the two bass levels cross at the midpoint, where each is at
/// 0.707 of unity. `from` for the incoming bass is 0 (it is brought in), so a cancel restores it
/// to silence; see [`StemFadeRunner::cancel`].
pub fn bass_swap(out_deck: u8, in_deck: u8, at_beat: f64, length_beats: f64) -> StemFadePlan {
    let window = (at_beat, length_beats);
    StemFadePlan::new(vec![
        stem_fade(
            out_deck,
            BASS,
            StemParam::Volume,
            (1.0, 0.0),
            window,
            Curve::EqualPower,
        ),
        stem_fade(
            in_deck,
            BASS,
            StemParam::Volume,
            (0.0, 1.0),
            window,
            Curve::EqualPower,
        ),
    ])
}

/// Vocals out: the deck's vocals fade from unity to silence over the window.
pub fn vocals_out(deck: u8, at_beat: f64, length_beats: f64) -> StemFadePlan {
    StemFadePlan::new(vec![stem_fade(
        deck,
        VOCALS,
        StemParam::Volume,
        (1.0, 0.0),
        (at_beat, length_beats),
        Curve::EqualPower,
    )])
}

/// Drums only for `length_beats`: bass, vocals and other are muted at `at_beat` and unmuted at
/// `at_beat + length_beats`. Drums are left alone.
pub fn drums_only_bridge(deck: u8, at_beat: f64, length_beats: f64) -> StemFadePlan {
    let mut fades = Vec::with_capacity(6);
    for stem in [BASS, VOCALS, OTHER] {
        fades.push(stem_fade(
            deck,
            stem,
            StemParam::Mute,
            (0.0, 1.0),
            (at_beat, 0.0),
            Curve::Linear,
        ));
        fades.push(stem_fade(
            deck,
            stem,
            StemParam::Mute,
            (1.0, 0.0),
            (at_beat + length_beats, 0.0),
            Curve::Linear,
        ));
    }
    StemFadePlan::new(fades)
}

/// Filter sweep on one stem: the sweep position moves from `from` to `to` (-1 low-pass, 0 off,
/// +1 high-pass) linearly over the window.
pub fn filter_sweep(
    deck: u8,
    stem: u8,
    at_beat: f64,
    length_beats: f64,
    from: f64,
    to: f64,
) -> StemFadePlan {
    StemFadePlan::new(vec![stem_fade(
        deck,
        stem,
        StemParam::Filter,
        (from, to),
        (at_beat, length_beats),
        Curve::Linear,
    )])
}

/// The beat a deck is at, from its snapshot position and a beat lookup (for example
/// `|f| grid.beat_at_frame(f)`). `None` when the deck has no track loaded.
pub fn beat_of_deck(deck: &DeckSnapshot, beat_at_frame: impl Fn(i64) -> f64) -> Option<f64> {
    deck.loaded.then(|| beat_at_frame(deck.position_frames))
}

/// What to do with the stems when a plan is cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelMode {
    /// Send each started fade's `from` value back.
    Restore,
    /// Send nothing: the stems stay where the plan left them.
    Leave,
}

/// Why a fade was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The target deck has no stems loaded (`StemSnapshot::available` is false).
    NoStems,
    /// The fade's id does not validate (deck or stem index out of range).
    InvalidId,
}

/// What one [`StemFadeRunner::tick`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TickReport {
    /// Commands sent this tick.
    pub sent: usize,
    /// Fades skipped this tick, by index into the plan. A fade is reported once and then ignored.
    pub skipped: Vec<(usize, SkipReason)>,
}

/// Applies a [`StemFadePlan`] over control ticks. Control thread only.
#[derive(Debug)]
pub struct StemFadeRunner {
    plan: StemFadePlan,
    /// Per fade: the reason it was skipped, once it has been.
    skipped: Vec<Option<SkipReason>>,
    /// Per fade: whether a value has been sent for it (used by a restoring cancel).
    started: Vec<bool>,
    last_beat: Option<f64>,
    cancelled: bool,
}

impl StemFadeRunner {
    pub fn new(plan: StemFadePlan) -> Self {
        let n = plan.fades.len();
        Self {
            plan,
            skipped: vec![None; n],
            started: vec![false; n],
            last_beat: None,
            cancelled: false,
        }
    }

    pub fn plan(&self) -> &StemFadePlan {
        &self.plan
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// True when every fade has reached its end at `beat`, was skipped, or the plan was cancelled.
    pub fn is_finished(&self, beat: f64) -> bool {
        self.cancelled
            || self
                .plan
                .fades
                .iter()
                .enumerate()
                .all(|(i, f)| self.skipped[i].is_some() || beat >= f.end_beat())
    }

    /// One control tick at `beat`. Sends `Command::Param` for each fade that overlaps the beats since
    /// the previous tick, at its value at `beat`, after checking that the target deck has stems.
    /// `engine` is the snapshot read this tick. A backwards jump (seek) re-sends the fades that
    /// overlap the jumped-over span at their value at `beat`.
    pub fn tick(
        &mut self,
        beat: f64,
        engine: &EngineSnapshot,
        sink: &dyn CommandSink,
    ) -> Result<TickReport, CommandError> {
        let mut report = TickReport::default();
        if self.cancelled || beat.is_nan() {
            return Ok(report);
        }
        let range = match self.last_beat {
            None => f64::NEG_INFINITY..beat,
            Some(last) => last.min(beat)..last.max(beat),
        };
        for (i, fade) in self.plan.fades.iter().enumerate() {
            if self.skipped[i].is_some() || !fade.overlaps(&range) {
                continue;
            }
            let id = fade.id();
            if id.validate().is_err() {
                self.skipped[i] = Some(SkipReason::InvalidId);
                report.skipped.push((i, SkipReason::InvalidId));
                continue;
            }
            if !stems_available(engine, fade.deck, fade.stem) {
                self.skipped[i] = Some(SkipReason::NoStems);
                report.skipped.push((i, SkipReason::NoStems));
                continue;
            }
            sink.send(Command::Param {
                id,
                value: fade.value_at(beat),
            })?;
            self.started[i] = true;
            report.sent += 1;
        }
        self.last_beat = Some(beat);
        Ok(report)
    }

    /// Cancels the plan. With [`CancelMode::Restore`] it sends the `from` value of each fade that
    /// started, once per parameter. With [`CancelMode::Leave`] it sends nothing. Either way later
    /// ticks send nothing. Returns the number of commands sent. A second cancel does nothing.
    pub fn cancel(
        &mut self,
        mode: CancelMode,
        sink: &dyn CommandSink,
    ) -> Result<usize, CommandError> {
        if self.cancelled {
            return Ok(0);
        }
        self.cancelled = true;
        if mode == CancelMode::Leave {
            return Ok(0);
        }
        let started = &self.started;
        let restore = first_values(
            self.plan
                .fades
                .iter()
                .enumerate()
                .filter(|(i, _)| started[*i])
                .map(|(_, f)| f),
        );
        let n = restore.len();
        for (id, value) in restore {
            sink.send(Command::Param { id, value })?;
        }
        Ok(n)
    }
}

fn stems_available(engine: &EngineSnapshot, deck: u8, stem: u8) -> bool {
    engine
        .stems
        .get(usize::from(deck))
        .and_then(|stems| stems.get(usize::from(stem)))
        .is_some_and(|s| s.available)
}

#[cfg(test)]
mod tests;
