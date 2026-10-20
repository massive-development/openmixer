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

mod cancel;
mod host;
mod state;
mod steps;
#[cfg(test)]
mod tests;

use cancel::{involves, CancelReason};
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
use std::sync::Arc;

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
