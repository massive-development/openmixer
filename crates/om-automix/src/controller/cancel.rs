use super::*;
use std::fmt;

/// Why a plan stopped early. Shown in `last_error`.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum CancelReason {
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

impl AutomixController {
    pub(super) fn detect_input(
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
            // The outgoing deck stopping at its last frame is the end of the track, not a pause.
            if ended && w.id == play_id(run.out) {
                continue;
            }
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
    /// Stops a running plan and leaves the user in charge. See the module docs.
    pub(super) fn cancel_transition(
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
            self.put_back(run, &plan.restore_on_cancel, &reason, snap, sink);
        }
        // The crossfader routing is not a musical move: it goes back whatever `restore_on_cancel` says.
        self.put_back(run, &plan.route_restore, &reason, snap, sink);
        self.last_error = reason.to_string();
        self.suppressed = Some((run.out, run.out_len));
        self.progress = 0.0;
        self.plan_style = None;
        self.next_title.clear();
        self.run = None;
        self.state = AutomixState::Idle;
    }

    // ----- helpers -----

    /// Sends each `(id, value)` the transition changed back to its value before. The control that
    /// caused the cancel and any control the user has moved keep what the user set.
    fn put_back(
        &self,
        run: &Run,
        restore: &[(ParamId, f64)],
        reason: &CancelReason,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
    ) {
        let trigger = match reason {
            CancelReason::UserInput(id) => Some(id.as_str()),
            _ => None,
        };
        for (id, value) in restore {
            if !run.was_sent(id) || trigger == Some(id.to_string().as_str()) {
                continue;
            }
            let moved = run.watches.iter().find(|w| &w.id == id).is_some_and(|w| {
                read_param(snap, id).is_some_and(|v| (v - w.cur).abs() > self.cfg.eps)
            });
            if !moved {
                let _ = send(sink, id, *value);
            }
        }
    }

    /// Drops whatever is in flight without sending anything and goes to `state`.
    pub(super) fn drop_run(&mut self, state: AutomixState) {
        self.run = None;
        self.skip_pending = false;
        self.state = state;
        self.progress = 0.0;
        self.plan_style = None;
        self.next_title.clear();
    }

    /// The current track on `out` had its turn (failed or skipped): do not try it again.
    pub(super) fn fail_run(&mut self, out: u8, out_len: i64, message: &str) {
        self.last_error = message.to_owned();
        self.suppressed = Some((out, out_len));
        self.force_start = false;
    }

    /// Stops whatever runs, for a user cancel or `enable` going off.
    pub(super) fn abort(
        &mut self,
        reason: CancelReason,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
    ) {
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

/// True when input on `id` could affect the pair of decks automix is mixing.
pub(super) fn involves(run: &Run, id: &ParamId) -> bool {
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
