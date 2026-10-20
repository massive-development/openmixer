use super::*;

impl AutomixController {
    // ----- Idle -----

    pub(super) fn tick_idle(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        sink: &dyn CommandSink,
        host: &mut dyn AutomixHost,
    ) {
        self.progress = 0.0;
        self.plan_style = None;
        self.next_title.clear();
        self.next_bpm = None;
        self.next_key.clear();
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
        if (remaining > self.cfg.arm_beats || !self.cfg.autostart) && !force {
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
                self.next_bpm = track.bpm;
                self.next_key = track.key;
                self.last_error.clear();
                self.run = Some(run);
                self.force_start = force;
                self.state = AutomixState::Loading;
            }
            Err(error) => self.load_failed(&track.title, &error),
        }
    }

    pub(super) fn out_info(
        &mut self,
        deck: u8,
        len: i64,
        host: &mut dyn AutomixHost,
    ) -> Option<DeckInfo> {
        if let Some((d, l, info)) = &self.out_cache {
            if *d == deck && *l == len {
                return Some(info.clone());
            }
        }
        let info = host.deck_info(deck)?;
        // Only an analysed deck is worth caching: a track loaded before its analysis finished has no
        // grid yet, and the next tick must read it again rather than keep the grid-less copy.
        if info.grid.is_some() {
            self.out_cache = Some((deck, len, info.clone()));
        }
        Some(info)
    }

    // ----- Loading -----

    pub(super) fn tick_loading(
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

    pub(super) fn load_failed(&mut self, title: &str, error: &str) {
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
    pub(super) fn tick_loading_ready(
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

    pub(super) fn plan_options<'a>(
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

    pub(super) fn tick_armed(
        &mut self,
        now: f64,
        snap: &EngineSnapshot,
        host: &mut dyn AutomixHost,
    ) {
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

    pub(super) fn begin(&mut self, now: f64, snap: &EngineSnapshot, underruns: u64, mut run: Run) {
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
}
