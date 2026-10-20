//! [`AutomixController`]: the state machine that turns "the playing deck is nearly over" into a
//! finished transition, on the control thread.
//!
//! ```text
//! Idle -> Loading -> Armed -> Transitioning -> Cleanup -> Idle
//!   ^        |         |           |
//!   +--------+---------+-----------+   (cancel, user input, failure, disabled)
//! ```
//! - **Idle**: one deck plays and automix is on. When that deck has `arm_beats` left (or
//!   `automix.start` was pressed) the queue's next track is loaded on a free deck. A free deck that
//!   already holds a track the user cued is used as is.
//! - **Loading**: waiting for the host's load. A failed load is reported in `last_error` and the
//!   next track is tried on a later tick; after `max_load_failures` in a row automix turns off.
//! - **Armed**: the incoming deck is cued at its mix-in point and the plan is made. The transition
//!   begins when the outgoing deck's beat (from its snapshot position) reaches the plan's start.
//! - **Transitioning**: each tick the plan's values at the outgoing deck's current beat are sent as
//!   `Command::Param` (only values that changed). Because values are a pure function of the beat,
//!   a stalled tick just jumps to the right value and the last tick lands every control exactly.
//! - **Cleanup**: the outgoing deck stops, the incoming one takes the sync lead, outgoing EQ/FX
//!   moves go back. Then idle again; the stopped deck is "stale" and is loaded over next time.
//!
//! **Cancel rules.** The user always wins. While armed or transitioning, a cancel happens when
//! - a watched control no longer has the value automix last set (crossfader, either channel fader,
//!   EQ/filter/sync/play ids the plan drives), compared with `eps`, or the binding layer reports
//!   input on an involved id with [`AutomixController::notify_user_input`];
//! - either deck is paused, unloaded, loaded over or seeked (playhead movement the clock does not
//!   explain);
//! - the host reports an underrun ([`AutomixHost::underruns`] rose): the transition is cancelled
//!   and automix turns off;
//! - `automix.cancel` is pressed or `automix.enable` goes off.
//!
//! On a cancel the user's value stays (nothing is sent to the control that was moved, and the
//! crossfader, faders, play and seek are never put back). EQ, filter and FX moves go back to their
//! plan-time values when `restore_on_cancel` is on, and stem moves follow `stem_cancel`. The track
//! is not retried: automix waits for the next track on that deck.
//!
//! The controller only calls the [`CommandSink`] and the [`AutomixHost`]; it holds no lock and
//! never waits. Threads: control thread only.

mod host;
#[cfg(test)]
mod tests;

pub use host::{AutomixConfig, AutomixControl, AutomixHost, LoadPoll, NextTrack};

use crate::plan::{PlanOptions, Style, TransitionPlan};
use crate::planner::{DefaultPlanner, TransitionPlanner};
use crate::readback::read_param;
use crate::stem_fade::StemFadeRunner;
use crate::types::DeckInfo;
use om_core::param_id::{ChannelParam, DeckParam, MixerParam};
use om_core::{
    AutomixSnapshot, AutomixState, Command, CommandError, CommandSink, DeckSnapshot,
    EngineSnapshot, ParamId,
};
use std::fmt;
use std::sync::Arc;

/// Why a plan stopped early. Shown in `last_error`.
#[derive(Debug, Clone, PartialEq)]
enum CancelReason {
    UserInput(String),
    Seek(u8),
    Unloaded(u8),
    Underrun,
    UserCancel,
    Disabled,
    SinkClosed,
}

impl fmt::Display for CancelReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserInput(id) => write!(f, "cancelled: {id} moved"),
            Self::Seek(d) => write!(f, "cancelled: deck {d} seeked"),
            Self::Unloaded(d) => write!(f, "cancelled: deck {d} stopped or reloaded"),
            Self::Underrun => f.write_str("cancelled: audio underrun, automix is off"),
            Self::UserCancel => f.write_str("cancelled"),
            Self::Disabled => f.write_str("cancelled: automix turned off"),
            Self::SinkClosed => f.write_str("cancelled: engine closed"),
        }
    }
}

/// A control automix set, with the value before that, so one tick of lag (a sink that applies
/// later) does not look like input.
#[derive(Debug, Clone)]
struct Watch {
    id: ParamId,
    cur: f64,
    prev: f64,
    /// The run's tick count when `cur` was set.
    set_at: u64,
}

/// The deck pair and the plan of one automix run.
struct Run {
    out: u8,
    inc: u8,
    out_info: DeckInfo,
    inc_info: Option<DeckInfo>,
    title: String,
    plan: Option<TransitionPlan>,
    stems: Option<StemFadeRunner>,
    watches: Vec<Watch>,
    out_len: i64,
    inc_len: i64,
    cue_frame: i64,
    /// When the run was armed (control clock seconds); the cue grace is measured from here.
    armed_at: f64,
    last_beat: f64,
    last_in_beat: Option<f64>,
    last_now: f64,
    load_started: f64,
    underruns: u64,
    /// Last value sent to each id that has no watch (FX, which the snapshot does not carry).
    unwatched: Vec<(ParamId, f64)>,
    /// Every id automix has sent a plan value to.
    sent: Vec<ParamId>,
    /// Ticks run in `Transitioning`.
    ticks: u64,
}

pub struct AutomixController {
    cfg: AutomixConfig,
    control: Arc<AutomixControl>,
    planner: Box<dyn TransitionPlanner + Send>,
    state: AutomixState,
    run: Option<Run>,
    /// The deck whose current track already got its turn (planned, cancelled or failed): automix
    /// does not retry it.
    suppressed: Option<(u8, i64)>,
    /// A deck automix stopped: its track is finished and the deck can be loaded over.
    stale_deck: Option<u8>,
    out_cache: Option<(u8, i64, DeckInfo)>,
    force_start: bool,
    pending_input: Option<String>,
    load_failures: u32,
    cleanup_left: u32,
    last_error: String,
    next_title: String,
    progress: f64,
    remaining_beats: Option<f64>,
    plan_style: Option<Style>,
}

impl AutomixController {
    pub fn new(cfg: AutomixConfig, control: Arc<AutomixControl>) -> Self {
        Self::with_planner(cfg, control, Box::new(DefaultPlanner))
    }

    pub fn with_planner(
        cfg: AutomixConfig,
        control: Arc<AutomixControl>,
        planner: Box<dyn TransitionPlanner + Send>,
    ) -> Self {
        Self {
            cfg,
            control,
            planner,
            state: AutomixState::Idle,
            run: None,
            suppressed: None,
            stale_deck: None,
            out_cache: None,
            force_start: false,
            pending_input: None,
            load_failures: 0,
            cleanup_left: 0,
            last_error: String::new(),
            next_title: String::new(),
            progress: 0.0,
            remaining_beats: None,
            plan_style: None,
        }
    }

    pub fn state(&self) -> AutomixState {
        self.state
    }

    /// The plan in flight or armed.
    pub fn plan(&self) -> Option<&TransitionPlan> {
        self.run.as_ref().and_then(|r| r.plan.as_ref())
    }

    /// The controller as readers see it (overlay it on the engine snapshot).
    pub fn snapshot(&self) -> AutomixSnapshot {
        AutomixSnapshot {
            enabled: self.control.is_enabled(),
            state: self.state,
            style: self.control.style(),
            plan_style: self.plan_style,
            progress: self.progress,
            next_track_title: self.next_title.clone(),
            remaining_beats: self.remaining_beats,
            last_error: self.last_error.clone(),
        }
    }

    /// The binding layer (MIDI, UI) reports input on `id`. If it touches an involved control the
    /// plan is cancelled on the next tick. Do not call it for automix's own commands.
    pub fn notify_user_input(&mut self, id: &ParamId) {
        if matches!(id, ParamId::Automix(_)) {
            return;
        }
        if let Some(run) = &self.run {
            if involves(run, id) {
                self.pending_input = Some(id.to_string());
            }
        }
    }

    /// One control tick. `now` is seconds on any monotonic clock; `snap` is this tick's engine
    /// snapshot.
    pub fn tick(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
        host: &mut dyn AutomixHost,
    ) {
        if self.control.take_cancel() {
            self.force_start = false;
            self.abort(CancelReason::UserCancel, snap, sink);
        }
        if self.control.take_start() {
            self.force_start = true;
        }
        if !self.control.is_enabled() {
            self.force_start = false;
            self.abort(CancelReason::Disabled, snap, sink);
            self.remaining_beats = None;
            return;
        }
        if let Some(id) = self.pending_input.take() {
            if self.state == AutomixState::Transitioning {
                self.abort(CancelReason::UserInput(id), snap, sink);
                return;
            } else if matches!(self.state, AutomixState::Loading | AutomixState::Armed) {
                // The user is handling the pair: this track gets no automatic transition.
                if let Some(run) = &self.run {
                    self.suppressed = Some((run.out, run.out_len));
                }
                self.last_error = format!("stepped aside: {id} moved");
                self.drop_run(AutomixState::Idle);
                return;
            }
        }
        match self.state {
            AutomixState::Idle => self.tick_idle(now, snap, sink, host),
            AutomixState::Loading => self.tick_loading(now, snap, sink, host),
            AutomixState::Armed => self.tick_armed(now, snap, host),
            AutomixState::Transitioning => self.tick_transition(now, snap, sink, host),
            AutomixState::Cleanup => {
                self.cleanup_left = self.cleanup_left.saturating_sub(1);
                if self.cleanup_left == 0 {
                    self.drop_run(AutomixState::Idle);
                }
            }
        }
    }

    // ----- Idle -----

    fn tick_idle(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
        host: &mut dyn AutomixHost,
    ) {
        self.progress = 0.0;
        self.plan_style = None;
        self.next_title.clear();
        let Some(out) = playing_deck(snap) else {
            self.remaining_beats = None;
            return;
        };
        let out_len = snap.decks[usize::from(out)].length_frames;
        if self
            .suppressed
            .is_some_and(|(d, l)| d == out && l == out_len)
        {
            self.remaining_beats = None;
            return;
        }
        if self.stale_deck == Some(out) {
            self.stale_deck = None;
        }
        let Some(info) = self.out_info(out, out_len, host) else {
            self.remaining_beats = None;
            return;
        };
        let Some(out_beat) = beat_of(&info, &snap.decks[usize::from(out)]) else {
            return;
        };
        let remaining = info.length_beats - out_beat;
        self.remaining_beats = Some(remaining.max(0.0));
        let force = self.force_start;
        if remaining > self.cfg.arm_beats && !force {
            return;
        }
        let Some(inc) = partner_deck(snap, out) else {
            self.fail_run(out, out_len, "no free deck for the next track");
            return;
        };
        self.force_start = false;
        let mut run = Run {
            out,
            inc,
            out_info: info,
            inc_info: None,
            title: String::new(),
            plan: None,
            stems: None,
            watches: Vec::new(),
            out_len,
            inc_len: 0,
            cue_frame: 0,
            armed_at: now,
            last_beat: out_beat,
            last_in_beat: None,
            last_now: now,
            load_started: now,
            underruns: 0,
            unwatched: Vec::new(),
            sent: Vec::new(),
            ticks: 0,
        };
        let partner = &snap.decks[usize::from(inc)];
        if partner.loaded && self.stale_deck != Some(inc) {
            // The user cued a track on the free deck: mix into it as it is.
            run.title = host.deck_title(inc);
            self.next_title.clone_from(&run.title);
            self.run = Some(run);
            self.state = AutomixState::Loading;
            self.tick_loading_ready(now, snap, sink, host, force);
            return;
        }
        let Some(track) = host.next_track(out) else {
            self.fail_run(out, out_len, "no track in the queue");
            return;
        };
        match host.start_load(inc, &track) {
            Ok(()) => {
                run.title = track.title.clone();
                self.next_title = track.title;
                self.last_error.clear();
                self.run = Some(run);
                self.force_start = force;
                self.state = AutomixState::Loading;
            }
            Err(error) => self.load_failed(&track.title, &error),
        }
    }

    fn out_info(&mut self, deck: u8, len: i64, host: &mut dyn AutomixHost) -> Option<DeckInfo> {
        if let Some((d, l, info)) = &self.out_cache {
            if *d == deck && *l == len {
                return Some(info.clone());
            }
        }
        let info = host.deck_info(deck)?;
        self.out_cache = Some((deck, len, info.clone()));
        Some(info)
    }

    // ----- Loading -----

    fn tick_loading(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
        host: &mut dyn AutomixHost,
    ) {
        let Some(run) = &self.run else {
            self.state = AutomixState::Idle;
            return;
        };
        let (out, inc, started) = (run.out, run.inc, run.load_started);
        if !deck_ok(snap, out, run.out_len, true) {
            self.drop_run(AutomixState::Idle);
            return;
        }
        match host.poll_load(inc) {
            LoadPoll::Pending => {
                if now - started > self.cfg.load_timeout_s {
                    let title = self.next_title.clone();
                    self.load_failed(&title, "timed out");
                }
            }
            LoadPoll::Failed(error) => {
                let title = self.next_title.clone();
                self.load_failed(&title, &error);
            }
            LoadPoll::Ready => {
                let force = std::mem::take(&mut self.force_start);
                self.tick_loading_ready(now, snap, sink, host, force);
            }
        }
    }

    fn load_failed(&mut self, title: &str, error: &str) {
        self.load_failures += 1;
        self.last_error = format!("could not load {title}: {error}");
        self.drop_run(AutomixState::Idle);
        if self.load_failures >= self.cfg.max_load_failures {
            self.control.set_enabled(false);
            self.last_error
                .push_str("; automix is off after repeated failures");
            self.load_failures = 0;
        }
    }

    /// The incoming deck holds its track: cue it, plan, arm.
    fn tick_loading_ready(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
        host: &mut dyn AutomixHost,
        asap: bool,
    ) {
        let Some(mut run) = self.run.take() else {
            return;
        };
        let inc_snap = snap.decks[usize::from(run.inc)];
        let Some(mut inc_info) = host.deck_info(run.inc).filter(|_| inc_snap.loaded) else {
            let title = run.title.clone();
            self.run = None;
            self.state = AutomixState::Idle;
            self.load_failed(&title, "no analysis for the track");
            return;
        };
        // A track the user left at a cue point keeps it; a fresh one is cued at its mix-in.
        if inc_snap.position_frames > 0 {
            if let Some(grid) = &inc_info.grid {
                inc_info.mix_in = Some(grid.beat_at_frame(inc_snap.position_frames));
            }
        }
        let out_snap = snap.decks[usize::from(run.out)];
        let out_beat = beat_of(&run.out_info, &out_snap).unwrap_or(run.last_beat);
        let plan = {
            let opts = self.plan_options(snap, &run, &out_snap, out_beat, asap);
            self.planner
                .plan(&run.out_info, &inc_info, self.control.style(), &opts)
        };
        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => {
                self.last_error = format!("no transition: {error}");
                self.suppressed = Some((run.out, run.out_len));
                self.state = AutomixState::Idle;
                self.next_title.clear();
                self.force_start = false;
                return;
            }
        };
        run.cue_frame = if inc_snap.position_frames > 0 {
            inc_snap.position_frames
        } else {
            inc_info
                .grid
                .as_ref()
                .map_or(0, |g| g.frame_at_beat(plan.in_cue_beat))
        };
        if inc_snap.position_frames <= 0 && inc_snap.length_frames > 0 && run.cue_frame > 0 {
            let fraction = run.cue_frame as f64 / inc_snap.length_frames as f64;
            let seek = ParamId::Deck {
                deck: run.inc,
                p: DeckParam::Seek,
            };
            if let Err(error) = send(sink, &seek, fraction.clamp(0.0, 1.0)) {
                self.last_error = format!("could not cue the next track: {error}");
                self.state = AutomixState::Idle;
                return;
            }
        }
        run.inc_len = inc_snap.length_frames;
        run.inc_info = Some(inc_info);
        run.last_now = now;
        run.armed_at = now;
        self.plan_style = Some(plan.style);
        run.plan = Some(plan);
        self.load_failures = 0;
        self.last_error.clear();
        self.run = Some(run);
        self.state = AutomixState::Armed;
    }

    fn plan_options<'a>(
        &self,
        snap: &'a EngineSnapshot,
        run: &Run,
        out_snap: &DeckSnapshot,
        out_beat: f64,
        asap: bool,
    ) -> PlanOptions<'a> {
        PlanOptions {
            max_bpm_ratio: self.cfg.max_bpm_ratio,
            min_length_beats: self.cfg.min_length_beats,
            length_beats: self.cfg.length_beats,
            out_position_beat: out_beat,
            lead_beats: self.cfg.lead_beats,
            start_asap: asap,
            out_rate: if out_snap.rate > 0.0 {
                out_snap.rate
            } else {
                1.0
            },
            out_on_left: run.out.is_multiple_of(2),
            echo_unit: self.cfg.echo_unit,
            snapshot: Some(snap),
        }
    }

    // ----- Armed -----

    fn tick_armed(&mut self, now: f64, snap: &EngineSnapshot, host: &mut dyn AutomixHost) {
        let force = std::mem::take(&mut self.force_start);
        let Some(mut run) = self.run.take() else {
            self.state = AutomixState::Idle;
            return;
        };
        let out_snap = snap.decks[usize::from(run.out)];
        let inc_snap = snap.decks[usize::from(run.inc)];
        let grace = now - run.armed_at <= self.cfg.cue_grace_s;
        let cue_tolerance = (inc_snap.file_sample_rate.max(8000.0) * 0.25) as i64;
        let cue_moved = !grace && (inc_snap.position_frames - run.cue_frame).abs() > cue_tolerance;
        if !deck_ok(snap, run.out, run.out_len, true)
            || !deck_ok(snap, run.inc, run.inc_len, false)
            || cue_moved
        {
            // The user took over one of the decks: step aside.
            self.next_title.clear();
            self.plan_style = None;
            self.state = AutomixState::Idle;
            return;
        }
        let Some(out_beat) = beat_of(&run.out_info, &out_snap) else {
            self.run = Some(run);
            return;
        };
        self.remaining_beats = Some((run.out_info.length_beats - out_beat).max(0.0));
        if force {
            // `automix.start`: replan to begin at the first bar the lead allows.
            if let Some(inc_info) = run.inc_info.clone() {
                let opts = self.plan_options(snap, &run, &out_snap, out_beat, true);
                match self
                    .planner
                    .plan(&run.out_info, &inc_info, self.control.style(), &opts)
                {
                    Ok(plan) => {
                        self.plan_style = Some(plan.style);
                        run.plan = Some(plan);
                    }
                    Err(error) => {
                        self.last_error = format!("no transition: {error}");
                        self.suppressed = Some((run.out, run.out_len));
                        self.next_title.clear();
                        self.plan_style = None;
                        self.state = AutomixState::Idle;
                        return;
                    }
                }
            }
        }
        run.last_beat = out_beat;
        let start = run.plan.as_ref().map(|p| p.start_beat);
        if start.is_some_and(|s| out_beat >= s) {
            self.begin(now, snap, host.underruns(), run);
        } else {
            self.run = Some(run);
        }
    }

    fn begin(&mut self, now: f64, snap: &EngineSnapshot, underruns: u64, mut run: Run) {
        let Some(plan) = run.plan.as_ref() else {
            return;
        };
        // Watch what the user could move: the crossfader, both channel faders, the play flags and
        // every readable id the plan drives. The baseline is what they are now.
        let mut ids: Vec<ParamId> = vec![
            ParamId::Mixer(MixerParam::Crossfader),
            fader(run.out),
            fader(run.inc),
            play_id(run.out),
            play_id(run.inc),
        ];
        for id in plan.step_ids() {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        run.watches = ids
            .into_iter()
            .filter_map(|id| {
                read_param(snap, &id).map(|v| Watch {
                    id,
                    cur: v,
                    prev: v,
                    set_at: 0,
                })
            })
            .collect();
        run.stems = plan.stem_plan.clone().map(StemFadeRunner::new);
        run.last_now = now;
        run.underruns = underruns;
        self.run = Some(run);
        self.state = AutomixState::Transitioning;
    }

    // ----- Transitioning -----

    fn tick_transition(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
        host: &mut dyn AutomixHost,
    ) {
        let (Some(mut run), true) = (self.run.take(), self.state == AutomixState::Transitioning)
        else {
            self.state = AutomixState::Idle;
            return;
        };
        let Some(plan) = run.plan.take() else {
            self.state = AutomixState::Idle;
            return;
        };
        run.ticks += 1;
        let out_snap = snap.decks[usize::from(run.out)];
        let Some(beat) = beat_of(&run.out_info, &out_snap) else {
            let reason = CancelReason::Unloaded(run.out);
            self.cancel_transition(&mut run, &plan, reason, snap, sink);
            return;
        };
        if host.underruns() > run.underruns {
            self.cancel_transition(&mut run, &plan, CancelReason::Underrun, snap, sink);
            self.control.set_enabled(false);
            return;
        }
        let ended = !out_snap.playing && beat >= run.out_info.length_beats - 0.5;
        if let Some(reason) = self.detect_input(&run, snap, now, beat, ended) {
            self.cancel_transition(&mut run, &plan, reason, snap, sink);
            return;
        }

        // Drive: the plan's value at this beat for every control, sent when it changed.
        for (id, value) in plan.values_at(beat) {
            let known = run.watches.iter().find(|w| w.id == id).map(|w| w.cur);
            if known.is_some_and(|cur| cur == value) {
                continue;
            }
            if known.is_none() && run.last_sent(&id) == Some(value) {
                continue;
            }
            match send(sink, &id, value) {
                Ok(()) => run.record(id, value),
                Err(CommandError::Closed) => {
                    self.cancel_transition(&mut run, &plan, CancelReason::SinkClosed, snap, sink);
                    return;
                }
                Err(error) => self.last_error = format!("command failed: {error}"),
            }
        }
        if let Some(stems) = run.stems.as_mut() {
            match stems.tick(beat, snap, sink) {
                Ok(report) => {
                    if let Some((_, reason)) = report.skipped.first() {
                        self.last_error = format!("a stem move was skipped: {reason:?}");
                    }
                }
                Err(CommandError::Closed) => {
                    self.cancel_transition(&mut run, &plan, CancelReason::SinkClosed, snap, sink);
                    return;
                }
                Err(error) => self.last_error = format!("stem command failed: {error}"),
            }
        }
        run.last_beat = beat;
        run.last_now = now;
        if let Some(inc) = snap.decks.get(usize::from(run.inc)) {
            if let Some(info) = &run.inc_info {
                if inc.playing {
                    run.last_in_beat = beat_of(info, inc);
                }
            }
        }
        self.remaining_beats = Some((run.out_info.length_beats - beat).max(0.0));
        self.progress = if plan.length_beats > 0.0 {
            ((beat - plan.start_beat) / plan.length_beats).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let stems_done = run.stems.as_ref().is_none_or(|s| s.is_finished(beat));
        if beat >= plan.end_beat() - 1e-9 && stems_done {
            self.finish(&run, &plan, sink);
            return;
        }
        run.plan = Some(plan);
        self.run = Some(run);
    }

    /// Why the user must have taken over, if they did.
    fn detect_input(
        &self,
        run: &Run,
        snap: &EngineSnapshot,
        now: f64,
        beat: f64,
        ended: bool,
    ) -> Option<CancelReason> {
        let out = snap.decks[usize::from(run.out)];
        let inc = snap.decks[usize::from(run.inc)];
        if !out.loaded || out.length_frames != run.out_len || (!out.playing && !ended) {
            return Some(CancelReason::Unloaded(run.out));
        }
        if !inc.loaded || inc.length_frames != run.inc_len {
            return Some(CancelReason::Unloaded(run.inc));
        }
        let dt = (now - run.last_now).max(0.0);
        let slack = self.cfg.seek_slack_beats;
        let moved = beat - run.last_beat;
        let expected = dt * run.out_info.bpm * out.rate.abs() / 60.0;
        if !ended && (moved < -0.5 || moved > expected * 1.5 + slack) {
            return Some(CancelReason::Seek(run.out));
        }
        if let (Some(prev), Some(info)) = (run.last_in_beat, &run.inc_info) {
            if let Some(now_beat) = beat_of(info, &inc).filter(|_| inc.playing) {
                let moved = now_beat - prev;
                let expected = dt * info.bpm * inc.rate.abs() / 60.0;
                if moved < -0.5 || moved > expected * 1.5 + slack {
                    return Some(CancelReason::Seek(run.inc));
                }
            }
        }
        for w in &run.watches {
            let Some(v) = read_param(snap, &w.id) else {
                continue;
            };
            let lagging =
                run.ticks.saturating_sub(w.set_at) <= 1 && (v - w.prev).abs() <= self.cfg.eps;
            if (v - w.cur).abs() > self.cfg.eps && !lagging {
                return Some(CancelReason::UserInput(w.id.to_string()));
            }
        }
        None
    }

    /// The plan ran to its end: every control is at its final value. Apply the end moves.
    fn finish(&mut self, run: &Run, plan: &TransitionPlan, sink: &dyn CommandSink) {
        for (id, value) in &plan.cleanup {
            if let Err(error) = send(sink, id, *value) {
                self.last_error = format!("cleanup command failed: {error}");
            }
        }
        self.stale_deck = Some(run.out);
        self.progress = 1.0;
        self.remaining_beats = None;
        self.cleanup_left = self.cfg.cleanup_ticks.max(1);
        self.next_title.clear();
        self.state = AutomixState::Cleanup;
        self.run = None;
    }

    /// Stops a running plan and leaves the user in charge. See the module docs.
    fn cancel_transition(
        &mut self,
        run: &mut Run,
        plan: &TransitionPlan,
        reason: CancelReason,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
    ) {
        if let Some(stems) = run.stems.as_mut() {
            let _ = stems.cancel(self.cfg.stem_cancel, sink);
        }
        if self.cfg.restore_on_cancel {
            let trigger = match &reason {
                CancelReason::UserInput(id) => Some(id.as_str()),
                _ => None,
            };
            for (id, value) in &plan.restore_on_cancel {
                if !run.was_sent(id) || trigger == Some(id.to_string().as_str()) {
                    continue;
                }
                // A control the user has moved keeps the user's value.
                let moved = run.watches.iter().find(|w| &w.id == id).is_some_and(|w| {
                    read_param(snap, id).is_some_and(|v| (v - w.cur).abs() > self.cfg.eps)
                });
                if !moved {
                    let _ = send(sink, id, *value);
                }
            }
        }
        self.last_error = reason.to_string();
        self.suppressed = Some((run.out, run.out_len));
        self.progress = 0.0;
        self.plan_style = None;
        self.next_title.clear();
        self.run = None;
        self.state = AutomixState::Idle;
    }

    // ----- helpers -----

    /// Drops whatever is in flight without sending anything and goes to `state`.
    fn drop_run(&mut self, state: AutomixState) {
        self.run = None;
        self.state = state;
        self.progress = 0.0;
        self.plan_style = None;
        self.next_title.clear();
    }

    /// The current track on `out` had its turn (failed or skipped): do not try it again.
    fn fail_run(&mut self, out: u8, out_len: i64, message: &str) {
        self.last_error = message.to_owned();
        self.suppressed = Some((out, out_len));
        self.force_start = false;
    }

    /// Stops whatever runs, for a user cancel or `enable` going off.
    fn abort(&mut self, reason: CancelReason, snap: &EngineSnapshot, sink: &dyn CommandSink) {
        match self.state {
            AutomixState::Transitioning => {
                if let Some(mut run) = self.run.take() {
                    if let Some(plan) = run.plan.take() {
                        self.cancel_transition(&mut run, &plan, reason, snap, sink);
                        return;
                    }
                }
                self.drop_run(AutomixState::Idle);
            }
            AutomixState::Idle => {
                if reason == CancelReason::UserCancel {
                    // Skip the transition that would come for the track playing now.
                    if let Some(out) = playing_deck(snap) {
                        let len = snap.decks[usize::from(out)].length_frames;
                        self.suppressed = Some((out, len));
                        self.last_error = reason.to_string();
                    }
                }
            }
            _ => {
                if reason == CancelReason::UserCancel {
                    if let Some(run) = &self.run {
                        self.suppressed = Some((run.out, run.out_len));
                    }
                    self.last_error = reason.to_string();
                }
                self.drop_run(AutomixState::Idle);
            }
        }
    }
}

impl Run {
    fn record(&mut self, id: ParamId, value: f64) {
        if let Some(w) = self.watches.iter_mut().find(|w| w.id == id) {
            w.prev = w.cur;
            w.cur = value;
            w.set_at = self.ticks;
        } else {
            self.unwatched.retain(|(i, _)| i != &id);
            self.unwatched.push((id.clone(), value));
        }
        if !self.sent.contains(&id) {
            self.sent.push(id);
        }
    }

    fn last_sent(&self, id: &ParamId) -> Option<f64> {
        self.unwatched
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, v)| *v)
    }

    fn was_sent(&self, id: &ParamId) -> bool {
        self.sent.contains(id)
    }
}

fn send(sink: &dyn CommandSink, id: &ParamId, value: f64) -> Result<(), CommandError> {
    sink.send(Command::Param {
        id: id.clone(),
        value,
    })
}

fn fader(deck: u8) -> ParamId {
    ParamId::Mixer(MixerParam::Channel {
        ch: deck,
        p: ChannelParam::Fader,
    })
}

fn play_id(deck: u8) -> ParamId {
    ParamId::Deck {
        deck,
        p: DeckParam::Play,
    }
}

/// The beat a deck is at on its own grid, when it has a track and a grid.
fn beat_of(info: &DeckInfo, deck: &DeckSnapshot) -> Option<f64> {
    let grid = info.grid.as_ref()?;
    deck.loaded
        .then(|| grid.beat_at_frame(deck.position_frames))
}

/// The one deck that plays, when exactly one does. Two playing decks mean the user is mixing.
fn playing_deck(snap: &EngineSnapshot) -> Option<u8> {
    let mut playing = snap
        .decks
        .iter()
        .enumerate()
        .filter(|(_, d)| d.loaded && d.playing)
        .map(|(i, _)| i);
    let first = playing.next()?;
    playing.next().is_none().then(|| u8::try_from(first).ok())?
}

/// The deck the next track goes to: the other half of the pair when it is free, else any free deck.
fn partner_deck(snap: &EngineSnapshot, out: u8) -> Option<u8> {
    let free = |i: usize| i != usize::from(out) && !snap.decks[i].playing;
    let pair = usize::from(out ^ 1);
    let pick = if pair < snap.decks.len() && free(pair) {
        Some(pair)
    } else {
        (0..snap.decks.len()).find(|i| free(*i))
    };
    pick.and_then(|i| u8::try_from(i).ok())
}

/// The deck still holds the track it had (same length) and is in the expected play state.
fn deck_ok(snap: &EngineSnapshot, deck: u8, len: i64, playing: bool) -> bool {
    snap.decks
        .get(usize::from(deck))
        .is_some_and(|d| d.loaded && d.length_frames == len && d.playing == playing)
}

/// True when input on `id` could affect the pair of decks automix is mixing.
fn involves(run: &Run, id: &ParamId) -> bool {
    let decks = [run.out, run.inc];
    match id {
        ParamId::Mixer(MixerParam::Channel { ch, .. }) => decks.contains(ch),
        ParamId::Mixer(MixerParam::Crossfader | MixerParam::CrossfaderCurve) => true,
        ParamId::Deck { deck, .. } | ParamId::Stem { deck, .. } => decks.contains(deck),
        ParamId::Fx {
            rack: om_core::param_id::FxRack::Channel(ch),
            ..
        } => decks.contains(ch),
        ParamId::Sync(_) => true,
        _ => false,
    }
}
