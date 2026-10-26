# UI faders, segmented meters, pitch and crossfader marks: plan

Date: 2026-10-24. Branch: `feat/ui-redesign`. Source research: `docs/design/research/faders-meters.md` (sections cited as R§n). Constraints: no new dependencies, do not touch `Cargo.lock`, ADR 0001 (conventions only, no vendor artwork).

## 0. Ownership

This plan OWNS:
- NEW `crates/om-ui-gpui/src/fader_paint.rs` (taper maths, tick model, cap/track/tick painting, pitch notch, crossfader marks)
- NEW `crates/om-ui-gpui/src/meter_paint.rs` (segment model, colour thresholds, segmented painter, clip lamp)
- `slider`, `slider_with_popup`, `stereo_meter`, `BarState` in `crates/om-ui-gpui/src/controls.rs` (bodies become thin wrappers over the new modules; signatures stay stable so callers compile unchanged until Phase 5)
- `pub mod fader_paint; pub mod meter_paint;` in `crates/om-ui-gpui/src/lib.rs` (one-line additions, sequenced with Task 1)

SHARED files (minimal call-site edits, ALL sequenced in Phase 5, last): `mixer_view.rs`, `drag.rs`, `om-ui/src/widgets/{master,crossfader,pitch}.rs` layout, `om-ui/src/theme/tokens.rs` defaults, `om-dsp/src/meter.rs` defaults. Other plans in flight may touch these; rebase before Phase 5 and keep each edit to the line counts listed there.

## 1. Current state (verified in code)

- `controls.rs::slider_with_popup`: track `div` plus a 12 px thumb at a linear fraction. Vertical when `rect.h >= rect.w`. No ticks, no cap anatomy.
- `controls.rs::stereo_meter`: continuous bar per side via `MeterScale::runs`, hold tick, clip lamp, dB labels via `fitting_marks`. Tokens `meter_segment` (4) and `meter_segment_gap` (1) exist in `theme/tokens.rs` and `theme/json.rs` and are used nowhere else.
- Stored fader semantics: UI `Ranged` value (0..1 channel, 0..2 master/cue) is sent straight to `MixerControl::set_fader` / `set_master_gain`, which store it as LINEAR gain (`control.rs` lines 51, 87). `drag.rs::fraction()` maps pointer fraction to value linearly. MIDI maps, saved params and tests (`drag.rs::a_channel_fader_follows_the_pointer_through_the_engine`) all assume this.
- Ballistics: `om-dsp::MeterBallistics` with exponential release, `release_seconds` default 0.3 (token `meter_release_seconds` 0.3). Test `attack_is_instant_and_release_is_exponential` pins current behaviour.

## 2. The taper decision (the critical risk)

R§5 risk 1: a dB-law fader changes the meaning of every stored fader value, MIDI map and test (0.5 was -6 dB; under the proposed law it would be about -15 dB).

Options considered:
1. Engine-side dB law (R§3 proposal). Highest risk: reinterprets saved mixes and MIDI CC-to-gain; needs a versioned migration; touches engine hot path.
2. Versioned migration only. Safe if done right, but still changes what every old value sounds like unless the old value is converted, and MIDI hardware CC positions would then map to different gains.
3. CHOSEN: keep the stored parameter exactly as is (linear gain), and apply the taper ONLY to the cap's on-screen position and the pointer-to-value mapping, in the UI. The cap position `pos` (0..1 of travel) maps to stored gain `g = law(pos)`; the inverse maps a stored gain to a cap position. Sound for any stored value is bit-identical to today.

Why option 3 is lowest risk:
- No engine change, no param-meaning change, no migration, no saved-file change. A mix saved yesterday sounds the same today; the cap simply sits where that gain belongs on a dB scale.
- MIDI: a CC still produces the same gain as before (CC maps to the stored value, not to cap position). The cap moves non-linearly when a CC sweeps, which is cosmetic. Soft takeover (out of scope) compares stored values, so it is unaffected.
- Reversible: the law is one pure function pair behind a `FaderLaw` enum; `Linear` is a kept variant, so a theme/pref switch or a revert is a one-line change.
- Only UI behaviour that changes is the drag feel, which is the purpose of the work.

Consequences and honest limits:
- Channel fader max is gain 1.0, so channel unity is the TOP of travel (0 dB at pos 1.0), not the R§3 "3/4 up with +6 above". Ticks: 0, -6, -12, -24, -48 (-inf at bottom). Master max is 2.0 (+6 dB), so master gets R§3's anatomy: +6 at top, 0 dB at pos 0.75.
- Stored values remain linear, so the law is: dB-linear section from top down to a floor, then a linear ramp to zero gain at the bottom (R§3 shape). Concretely, `ChannelLaw`: pos 1.0 = 0 dB, pos 0.2 = -48 dB, linear in dB between, then gain linear 0..10^(-48/20) over pos 0..0.2. `MasterLaw`: pos 1.0 = +6 dB, 0.75 = 0 dB, 0.25 = -30 dB (linear in dB), ramp to 0 below 0.25.
- The existing layout test and the value readout (`Readout::Gain.text`) already show dB of the stored gain; they stay correct because the stored value is unchanged.
- Double-click reset targets stay on the stored default (channel default and master default as defined now); only the cap position on screen moves. If today's channel default is not 1.0, its tick reads where it truly is (honest).
- Alternative display-only fallback if Phase 5 drag edits collide with other work: draw ticks at the true dB positions of the existing LINEAR travel (0 dB at p=1.0, -6 at 0.5, -12 at 0.25, -20 at 0.1). This needs zero drag.rs changes and zero behaviour change, but bunches ticks at the bottom. Ship this first (Phase 2 default `FaderLaw::Linear`) and flip to the tapered law in Phase 5 (one-line switch plus drag.rs edits).

An engine-level law (option 1/2) is NOT in this plan except as note in Phase 6.

## 3. Phases and ordered tasks

All commands from repo root. Run `cargo test -p om-ui-gpui` (and `-p om-ui`, `-p om-dsp` where named). Tests come first in every task: write, run, see red, implement, see green, `cargo fmt`, `cargo clippy -p om-ui-gpui -- -D warnings`. Commit per task. Never `git add Cargo.lock` (it is already dirty in the working tree; stage files by explicit path).

### Phase 1: pure maths (no GPUI, fully unit tested)

Task 1.1. Create `fader_paint.rs` skeleton plus `lib.rs` module lines, with `#[cfg(test)] mod tests`. Tests first:
- `channel_law_pos_one_is_unity_and_pos_zero_is_silent`
- `master_law_unity_at_three_quarters_and_plus_six_at_top`
- `law_round_trips`: for 200 positions, `pos_of(gain_at(pos))` equals pos within 1e-4, for both laws and `Linear`
- `law_is_monotonic_and_continuous_at_the_ramp_join`
- `linear_law_is_the_identity` (guards the fallback)
- `gain_to_db_floor`: gain 0 gives `f32::NEG_INFINITY`; readout string `-inf` is produced by a helper `db_text(gain)`.
Implement `enum FaderLaw { Linear, Channel, Master }` with `gain_at(pos: f32) -> f32`, `pos_of(gain: f32) -> f32`, `db_at(pos)`, `pos_at_db(db)`. Pure f32, no allocation.

Task 1.2. Tick model tests then code: `ticks(law, travel_px) -> Vec<Tick { pos, db, major, label }>`.
- Channel ticks 0, -6, -12, -24, -48; master +6, 0, -6, -12, -24, -48 (R§3 numbers; mark as proposal in doc comment).
- `ticks_are_dropped_when_closer_than_12px` (reuse the `MIN_MARK_SPACING` idea from `om-ui/src/meter.rs`); the unity tick is always kept and is the accent one.
- `labels_only_on_major_ticks` and `label_text_min_12px` (const `MIN_TEXT_PX`).

Task 1.3. Pitch and crossfader maths tests then code (same file):
- `pitch_notch_is_at_centre_of_travel` (rate 1.0 maps to pos 0.5 for every range in `om_ui::widgets::pitch::RANGES`)
- `pitch_readout_one_decimal_signed`: 1.012 shows `+1.2 %`, 1.0 shows `0.0 %`, 0.92 shows `-8.0 %` (check against existing `format` helpers first; reuse, do not duplicate)
- `pitch_minor_ticks_at_half_range`
- `crossfader_marks`: A at the left end, B at the right end, centre tick at 0.5.
Pitch snap zone (R§3 software detent of +-0.15 % of range) is NOT applied to stored values in this plan (it would change stored semantics); it is a visual notch only.

Task 1.4. Create `meter_paint.rs` with tests first for the segment model:
- `segment_count_for_height`: `n = floor((h + gap) / (segment + gap))` using `meter_segment` and `meter_segment_gap` tokens; 160 px gives 27 with 4+1, never zero for h >= 4, never panics for h < 4.
- `segment_top_db_is_linear_in_position`: segment i top dB spans floor..0 (floor from `meter_floor_db`).
- `segment_colour_thresholds`: using `MeterScale::zone` semantics, a segment whose top dB is `>= high_db` is red, `>= mid_db` amber, else green; boundary values exactly at -18/-6 (current tokens) and at -12/-3 (R§3 proposed) for both token sets.
- `lit_count_matches_level`: level 0 lights none, level 1 lights all, level 0.5 lights half, a level just above a segment's bottom lights it (round up so a visible signal always lights the first segment: `quiet_signal_lights_first_segment` at -47 dB).
- `hold_segment_index_matches_hold_level_and_has_zone_colour`.
- `runs_of_lit_segments_are_coalesced_per_zone`: given a lit count, `lit_runs()` returns at most 3 rects (one per zone), each with the segment gap pattern preserved (see perf, Task 4.3).
Implement `SegmentModel::new(tokens, height_px)`, `lit_runs(level) -> [Option<Run>; 3]`, `hold_index(hold)`. No gpui types in the model.

Task 1.5. Ballistics tests then code (in `om-dsp` where the maths lives, `crates/om-dsp/src/meter.rs`, an additive change):
- `release_is_13_db_per_second`: from 0 dB with zero input, after 1.0 s the level is within 0.5 dB of -13 dB (R§3). Needs a dB-linear release (constant dB/s) or tune `release_seconds`. Prefer making the decay constant in dB/s via a new `MeterParams::release_db_per_second` with `Default` 13.0; keep `release_seconds` field only if other code reads it (grep first; adjust `om-ui/src/meter.rs::meter_params` and `tokens.rs`).
- `attack_stays_instant`, `release_is_frame_rate_independent` (existing, must still pass), `hold_one_second_then_falls`, `clip_latches_two_seconds_or_until_click`.
- Update `attack_is_instant_and_release_is_exponential` deliberately and note the change in the commit body.
Migration note: this changes only display ballistics; no saved data.

### Phase 2: fader painting (owned files only)

Task 2.1. Painter `fader_paint::fader(t, rect, face: FaderFace, cx, ...)` building on `place`, `skin`, `border` from `paint.rs`. Anatomy from R§3: slot well (4 px, `fader.track` skin darker inset), ticks on the left (minor 6 px, major 10 px, 1 px high) via `place(Rect)` divs, labels 12 px for majors, cap 22x36 (channel) / 26x40 (master) with a 2 px centre line, accent highlight line while dragging. Colours from `LookAndFeel` / tokens (R colours section is for reference; resolve through theme so daylight works). Test (pure): `fader_geometry` returns cap y for a given pos and travel = slot length - cap height, never leaves the slot (`cap_stays_inside_track` at pos 0 and 1), and ticks align to the cap centre at the same pos (`tick_y_equals_cap_centre_y`). That alignment test is the guard against the classic off-by-half-cap bug.

Task 2.2. Wire `controls.rs::slider_with_popup` for vertical sliders to call `fader_paint` with `FaderLaw::Linear` (no behaviour change); horizontal sliders (crossfader, phones mix and volume) keep the old thumb path until Task 3.3. `draggable(...)` and the popup are unchanged, so `drag.rs` tests stay green.

Task 2.3. Pitch fader: `FaderFace::pitch(range)` adds the centre notch (12 px, accent, at pos 0.5), minor ticks at half range, and a readout above (`+1.2 %`, 12 px) which replaces or agrees with the existing `0.0%` text in `deck_view.rs` (that is a Phase 5 call-site edit; until then the pitch fader only gains notch and ticks). Reset stays double-click via `draggable`.

Task 2.4. Crossfader: `controls.rs::mark` already draws A and B; the new painter adds a centre tick (10 px) and end ticks inside the slot, and a 28x20 cap. Leave the existing `mark` calls in `mixer_view.rs` (they remain valid) so the minimal edit set stays small.

Visual check (each of 2.x): `tools/screenshots.sh main-dark` then `main-daylight`; open `docs/screenshots/main-dark.png` and confirm cap, ticks, labels not clipped at 1x. Do not commit regenerated PNGs until Phase 7 (the working tree already has an uncommitted `main-dark.png` change; avoid mixing).

### Phase 3: meter painting

Task 3.1. `meter_paint::segmented_meter(t, rect, labels, bars, cx, on_click)` implementing R§3 meter table: 12 px per side, 2 px between, segment from `t.meter_segment` and `t.meter_segment_gap`, unlit track drawn as ONE div with the Off colour (`m.well`), lit zones as up to 3 coalesced runs per side, hold tick as one 1-segment div in its zone colour, clip lamp above the bar (`m.lamp_on` / `m.lamp_off`, already tokens), click clears lamps (`on_click`, unchanged signature). The segment gaps are produced by dark 1 px gap rects over the lit runs (see Task 4.3 for the budget) so the element count stays low.

Task 3.2. `controls.rs::stereo_meter` body is replaced by a call to `segmented_meter`; `BarState` unchanged (level, hold, clipping) so `mixer_view.rs::bar_state` compiles unchanged. Keep `fitting_marks` dB labels for non-master and add the master peak numeric readout (`-2.3`, 12 px) as an optional extra argument defaulting to none: add `stereo_meter_with_readout` in `meter_paint.rs` rather than changing the existing signature.

Task 3.3. Master meter with clip lamp: master already receives `BarState::clipping`; ensure the lamp is taller on master (R: 6 px) via `MeterFace { lamp_h }` and has an off colour distinct from the bar well. Test: `master_clip_lamp_lights_when_either_side_clips` over the model.

Token handling: R§3 suggests `meter_mid_db` -12 and `meter_high_db` -3. Changing those defaults shifts dB label positions and every colour boundary, so do it only in Phase 5 with the screenshot diff, not here. Phase 1 tests cover both token sets.

Visual check: `tools/screenshots.sh main-dark main-daylight`, plus run the mock device with a test tone if the demo supports it (`--mock-device` is silent in screenshot scenes, so levels will be empty; verify lit segments using a debug unit test that renders a model at 0.3/0.7/1.0 and by running the app with a real or loopback device manually). Check amber on the light background for >= 3:1 (R§5 risk 7).

### Phase 4: performance (gate before Phase 5)

Task 4.1. Count: 8 meters in the mixer (4 channels maybe fewer in current layout, plus master) times 2 sides; each side is at most about 1 well + 3 runs + 1 hold + 1 lamp + gap overlays. Add a unit test `element_budget` in `meter_paint.rs` asserting a stereo meter emits <= 24 divs regardless of height, and a fader emits <= 24 (ticks plus labels). Total for the screen: under 300 elements including fader ticks, which matches the research estimate when meters were 60 divs each (that per-segment design is what we avoid).
Task 4.2. Allocation: models are computed from tokens and a level; return fixed arrays or small `Vec` reserved by the known bound. No per-frame `format!` for tick labels: cache the label strings in a `static` table or `const &str` per tick.
Task 4.3. Gap pattern: first implement gaps as N thin overlay rects only on the unlit track (drawn once, same each frame, cheap); if a lit run needs gaps too and the budget test fails, switch to the single-div approach in R§5 risk 2 (draw gaps over the whole bar). Measure with the existing `frame_stats.rs` / `debug_panel.rs` frame time readout before and after: run the app, open the debug panel, record median and p99 frame time with the old meter (git stash or checkout of the previous commit) and the new. Acceptance: no more than +0.5 ms median at 60 Hz and element count under 300 from the budget tests. If it fails, reduce to coalescing only.

### Phase 5: shared-file call-site edits (SEQUENCED LAST; keep each small)

Rebase on whatever has merged to `main` first, then in this order, one commit each, tests green after each:
1. `crates/om-ui-gpui/src/mixer_view.rs`: (a) pass `FaderLaw::Channel` / `FaderLaw::Master` where `slider_with_popup` is called for the channel fader (about line 166) and master (about line 256), via a new `fader_with_law(...)` in `fader_paint.rs` (about 2 lines each, or extend `slider_with_popup` with a defaulted wrapper); (b) swap `stereo_meter` for the readout variant on master only (about line 265). No layout code changes.
2. `crates/om-ui-gpui/src/drag.rs`: in `begin_drag`/`drag_move`, where the pointer fraction becomes a value via `fraction`/range, route Channel and Master targets through `FaderLaw::gain_at(frac)` and the inverse for display (`value_of`). Do NOT touch `apply`, `range_of`, `reset_target`, `default_of`. Add tests: `a_channel_fader_with_the_taper_gives_unity_at_the_top`, `a_stored_half_gain_still_reads_minus_six_db` (proves stored semantics unchanged), `a_double_click_still_resets_to_default`. Existing drag tests must pass unmodified except the ones that assert exact linear values from pointer positions; update only those, with a comment citing this plan.
3. Optional: shift-fine and scroll wheel on faders (R§3 Interaction), mirrored from `deck_view.rs`. Out of the minimum scope; do only if time remains.
4. `crates/om-ui/src/theme/tokens.rs`: `meter_mid_db` -12, `meter_high_db` -3 (R§3) only if screenshots justify it; `crates/om-ui/src/theme/json.rs` untouched (ranges already allow it). Update tests that hard-code -18/-6 (grep `meter_mid_db`).
5. `crates/om-ui/src/widgets/{master,crossfader,pitch}.rs` layout: taller track and wider gutter for ticks if the new painters need it (check `MasterLayout`, `CrossfaderLayout` fields). Keep within existing bounds (the 1280x800 window; see `docs/screenshots/main-dark.png`); the label gutter is 22 px, so the tick column needs about 14 px more on the left of each channel fader. If the strip cannot afford it, drop labels and keep unlabelled ticks plus the accent unity tick.
6. `deck_view.rs`: pitch readout text replaced by the `fader_paint` readout helper (about 3 lines).

### Phase 6: OPTIONAL engine-level work (separate, not required for this UI to ship)

Do not start until Phases 1 to 5 are merged. Each item needs its own plan and tests.
- Sharp-cut and reverse crossfader curves: extend `om-dsp::CrossfaderCurve` (add `Cut`, `Add`), param range `crossfader.curve` to 0..3, reverse toggle param, mapping editor entries (R§3, R§5 risk 4). Existing MIDI maps use 0/1; the new values are additive, so old maps keep working.
- RMS / true-peak / PFL meter: sample-accurate overs and a cue meter tap in `om-engine/src/mixer/` (R§5 risk 3).
- True engine dB law (option 1/2 above): only with a versioned parameter (`fader.law = 1`), a migration that converts old linear values at load (`new_pos = law.pos_of(old_gain)` so the stored value gains meaning while sound is unchanged), MIDI-map version bump, and golden audio tests. Needs sign-off; the UI-only taper delivers the visible benefit without it.
- Soft takeover and ghost tick (R§3, R§5 risk 6).

### Phase 7: verification and docs

1. `cargo test --workspace` (not `--all-features` unless the repo already does), `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`. Confirm `git status` shows `Cargo.lock` unchanged by you.
2. `tools/screenshots.sh` (all scenes; mock device, X11 display, ImageMagick needed). Inspect main-dark, main-daylight, main-automix, main-stems. Check: fader caps visible and inside the track at min and max (drag with the mouse in a manual session), ticks align to cap centre, meter segments crisp at 1x (and at 2x if an HiDPI display is available), pitch notch centred, crossfader A/B and centre tick visible, master clip lamp visible and clickable.
3. Commit regenerated screenshots in a separate `docs:` commit; the working tree currently has an unrelated modified `docs/screenshots/main-dark.png`, so regenerate on a clean base to avoid mixing.
4. `tools/check_docs_rust.py` if docs mention changed symbols.

## 4. Risks

1. Cap position law is cosmetic but changes drag feel; users used to linear travel will find the channel fader finer near the top. Mitigation: `FaderLaw::Linear` stays selectable and is the Phase 2 default; the flip is one commit.
2. `drag.rs` and `mixer_view.rs` are shared and likely edited by parallel redesign branches; Phase 5 is last on purpose, and the edits are scoped to named call sites.
3. Unsourced numbers (tick dB, 22x36 cap, 13 dB/s release, -12/-3 thresholds) are proposals from R; validate visually at 1x and 2x.
4. Space: the channel strip is already tight (see screenshot); ticks plus labels may not fit. Fallback: unlabelled ticks.
5. Peak-only engine meters understate overs (R§5 risk 3); the clip lamp reflects block peaks. Documented, deferred to Phase 6.
6. Release default change alters meter feel for existing users; it is display-only and covered by tests.
7. Daylight amber contrast needs a manual check.
8. Screenshots are silent (mock device), so meter segments must be verified with unit tests plus a manual live run.
