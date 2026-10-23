use super::*;

impl AutomixController {
    // ----- Transitioning -----

    pub(super) fn tick_transition(
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
        // `automix.skip`: the user's input is still checked below, then the plan lands at its end.
        let skip = std::mem::take(&mut self.skip_requested);
        let out_snap = snap.decks[usize::from(run.out)];
        let Some(actual) = beat_of(&run.out_info, &out_snap) else {
            let reason = CancelReason::Unloaded(run.out);
            self.cancel_transition(&mut run, &plan, reason, snap, sink);
            return;
        };
        if host.underruns() > run.underruns {
            self.cancel_transition(&mut run, &plan, CancelReason::Underrun, snap, sink);
            self.control.set_enabled(false);
            return;
        }
        let ended = !out_snap.playing && actual >= run.out_info.length_beats - 0.5;
        if let Some(reason) = self.detect_input(&run, snap, now, actual, ended) {
            self.cancel_transition(&mut run, &plan, reason, snap, sink);
            return;
        }
        let beat = if skip { plan.end_beat() } else { actual };

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
            if skip {
                self.last_error = "transition skipped".to_owned();
            }
            return;
        }
        run.plan = Some(plan);
        self.run = Some(run);
    }

    /// Why the user must have taken over, if they did.
    pub(super) fn finish(&mut self, run: &Run, plan: &TransitionPlan, sink: &dyn CommandSink) {
        for (id, value) in &plan.cleanup {
            if let Err(error) = send(sink, id, *value) {
                self.last_error = format!("cleanup command failed: {error}");
            }
        }
        self.stale_deck = Some((run.out, run.out_len, run.out_info.loaded_track_id));
        self.progress = 1.0;
        self.remaining_beats = None;
        self.cleanup_left = self.cfg.cleanup_ticks.max(1);
        self.next_title.clear();
        self.next_bpm = None;
        self.next_key.clear();
        self.state = AutomixState::Cleanup;
        self.run = None;
    }
}
