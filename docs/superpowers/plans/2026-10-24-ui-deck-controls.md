# Deck controls UI plan (round transport, lit toggles, named pads, loop controls, platter)

Status: merged in PR #115 (polish in PR #116).

Branch: `feat/ui-redesign`. Spec: `docs/design/research/deck-controls.md` (sections 2-5). Conventions only, no copied art or text (ADR 0001).

**Constraints.** No new dependencies; do not touch `Cargo.lock` (it is already modified in the working tree, leave it alone, never `git add -A`). This plan OWNS `crates/om-ui-gpui/src/deck_view.rs` and the NEW `crates/om-ui-gpui/src/platter_paint.rs`. Other plans may edit `waveform_paint.rs`, `mixer_view.rs`, `app.rs`, theme tokens; do not edit them except in the "Shared-file edits" list, which goes last.

## 0. Verified engine/model facts (checked against the source on 2026-10-24)

| Capability | Exists? | Evidence |
|---|---|---|
| Play, Cue (cue = stop and return), Rate, Keylock, Seek (`DeckParam::Seek`, `Deck::seek(frame)`) | Yes | `om-core/src/param_id/mod.rs` `DeckParam`; `om-engine/src/deck.rs:260` |
| Sync, **Tempo leader** (`DeckParam::SyncMaster`, `DeckState::SyncMaster`, `DeckSnapshot.sync_master`) | Yes, already in the model; only the UI hides it in the More drawer | `deck_control.rs:133`, `snapshot.rs:65` |
| Slip, Quantize | Yes | `DeckParam::Slip/Quantize` |
| Loop exit / reloop / quantized loop of N beats (`LoopQuantized(f64)`) | Yes | `loops.rs` (`halve`, `double`, `toggle`) |
| `Deck::set_loop(Loop{start,end})` with arbitrary frames | Engine yes, **no `DeckParam`** | `deck.rs:277`; `DeckParam` has no LoopIn/LoopOut |
| **Loop in / Loop out** | **No** | not in `DeckParam`, `DeckControl`, or `DeckSnapshot` (snapshot has `loop_start/loop_end` read-only) |
| **Beat jump** | **No** | no `jump` symbol in om-core/om-engine |
| **Scrub / nudge / pitch bend** | **No** | no scrub API; `Rate` is the only tempo path and it is the pitch fader's value, so a bend would fight the fader |
| Hot cue **names / per-pad colour** | **No** | `HotcueBank` stores frames only (`DeckSnapshot.hotcues: [Option<i64>; 8]`); colour is `CUE_PALETTE[index]` in `om-ui/src/waveform/markers.rs` |
| Track artwork in the UI | Not decoded in GPUI (`waveform_paint.rs` header: `RenderImage` needs the `image` crate = Cargo.lock change) | |

Re-verify in Task 0 with grep before starting; if any row changed, adjust.

## Part A: UI-only, ships now (no model, engine or om-ui edits)

Everything in A uses existing `DeckBinding`, `TransportPanel`, `HotcuePads`, `LoopControls`, `DeckSnapshot`. Cue names in A are derived defaults ("Cue 1".."Cue 8"); the pad shows the name, the set/empty state and colour from `HotcuePads::colour`.

Layout rule that avoids touching `om-ui` `DeckLayout`: every new rect is computed in two pure functions in the files this plan owns (`deck_view.rs::local_layout`, `platter_paint.rs::platter_geometry`), taking the existing `DeckLayout` rects as input. Controls column is ~230 px tall (`CONTROLS_HEIGHT`), too small for the 160 px platter: the platter is placed in the zoom region's space only when `l.zoom_visible` is false is NOT acceptable (it hides the waveform), so the decision is: platter lives **left of the pitch fader, as a "Platter" fourth tab** (mode held in a local `Shell`-independent `Cell` in `deck_view.rs` keyed by deck index, see Task 7). This avoids a `PadMode` edit now; the shared-file task at the end promotes it to a real `PadMode::Platter` only if wanted.

### Task 0: baseline and facts (no code)
1. `cargo test -p om-ui -p om-ui-gpui` green; `git status` shows only the pre-existing dirty files.
2. Grep the table above (`rg -n "jump|LoopIn|loop_in|scrub|nudge" crates/om-core crates/om-engine`); confirm still absent.
3. `tools/screenshots.sh main-dark`, view `docs/screenshots/main-dark.png`, keep a copy at `$TMPDIR/before.png` (do not commit screenshot changes until Task 9).

### Task 1 (tests first): pure style/state mapping in `deck_view.rs`
Add `#[cfg(test)] mod tests` in `deck_view.rs` for new pure fns, then implement:
- `fn transport_style(button, lit, deck_accent) -> ButtonStyle { shape: Round|Pill, fill, text, glow }`. Tests: Play and Cue are `Round` and 56 px; Sync, Tempo leader, Key lock, Snap to beat, Slip are `Pill`; Play lit = green fill and glyph/word switches Play to Pause; Cue held = amber; idle uses `well`; disabled (not `loaded`) gives 0.4 alpha and no hover; Sync lit = existing `SYNC_ON`.
- `fn pad_state(set: bool, active: bool, i: usize) -> PadStyle`. Tests: empty = dashed border, label "Set", no colour; set = `HotcuePads::colour(i)` at 85% alpha, label "Cue N", 3 px top tab; active = full alpha plus glow; text colour switches to black when luminance > 0.4 (use the existing contrast helper in `om-ui::theme`; assert >= 4.5:1 for all 8 palette entries on dark and daylight).
- `fn loop_view(frame: LoopFrame) -> LoopView`. Tests: label from `LoopFrame::label()`, Loop button lit iff `reloop_lit`, halve/double always enabled when loaded.
Run `cargo test -p om-ui-gpui` red then implement until green.

### Task 2: round Play and Cue
Replace the `tile()` call in `transport()` for Play/Cue with a `round_button()` helper (56 px, `rounded(px(28.))`, glyph + word under 12 px floor stays: word below glyph). Keep `TransportPanel::press` routing untouched. Rect: derive from `l.transport` in `local_layout()` (56 px circles, 24 px gap, 12 px to Sync); if `l.transport.h < 56`, fall back to 44 px circles (`compact`). Visual check Task 8.

### Task 3: lit-state toggles and promoted Tempo leader
Draw Sync, Tempo leader (tooltip "Other decks follow this deck's tempo (master)"), Key lock, Snap to beat, Slip as pills with the Task 1 style. Tempo leader/Key lock/Snap/Slip move out of the More tab into a toggle row only if `local_layout` finds room (>= 36 px); otherwise they stay in More with the new styling. Use `TransportButton::SECONDARY` and `TransportPanel::is_lit`; label overrides live in `deck_view.rs` (do NOT edit `transport.rs::label`; that is the shared-file task).

### Task 4: coloured named hotcue pads
Rewrite `hotcues()` body to use `pad_state`: set/empty/active from `deck.hotcues.lit()` plus `DeckSnapshot.hotcues[i].is_some()` and "active" = playhead within 1 frame-block of the cue. Name text "Cue N" (A); empty "Set". Keep `Modifiers`/`click` routing (shift/alt/ctrl clears). Min hit 44 px unchanged (existing `deck_widgets.rs` test stays green).

### Task 5: loop controls
Rewrite `loops()` as: `[halve] [length chip] [double]` stepper plus a separate Loop toggle (exit/reloop via `LoopControls::toggle`). Length chip shows `LoopFrame::label()`; pressing a number does nothing new. **Loop in / Loop out buttons render as disabled pills with tooltip "Needs engine support" only if product wants the layout reserved; default: omit until Part B.** (Do not fake them with `LoopQuantized`.)

### Task 6 (tests first): `platter_paint.rs` geometry and input maths
New module, `pub mod platter_paint;` added to `lib.rs` (shared-file list). Pure, unit-tested before any painting:
- `platter_geometry(rect) -> Platter { centre, radius, ring_outer, ring_inner, label_radius (0.56*r), bend_band_inner (0.85*r) }`. Tests: label = 56% of diameter; bend band is the outer 15%; scales 160 px to 112 px compact.
- `marker_angle(elapsed_s, rate, playing) -> f32`: 1 turn per 1.8 s at rate 1.0, holds when paused, scales with rate. Test wraparound and monotonic.
- `progress_sweep(position_frames, length_frames) -> f32` in 0..=360; test 0 length gives 0.
- `zone_at(point, platter) -> Zone::{Outside, Scratch, Bend}`; test boundaries.
- `angle_delta(prev, now) -> f32` handling the +-180 wrap; test across the seam.
- `scratch_target(start_frame, start_angle, now_angle, frames_per_turn) -> i64` clamped to the track; test clamping.

### Task 7: platter painting and input
- Paint with `PathBuilder` + `Window::paint_path` (`move_to/line_to/arc_to`), as `waveform_paint.rs` does. Disc, groove rings and tick marks built once per (size, theme) into a cached `Vec<Path>` held in a `RefCell` (same pattern as the overview cache); per frame rebuild only the marker (one small path) and the progress arc (arc segments, <= 120 vertices). Artwork: static centre label = deck-coloured circle with the deck letter (artwork is a stated risk: no rounded mask, no `image` dependency); a later task may add square-cropped art. Rotate only the ring ticks and marker, never the label.
- Input: mouse down in Scratch zone: remember position, `deck.transport` Play untouched; on move send `DeckParam::Seek` with `scratch_target` via the existing `DeckBinding` (throttle to one send per frame); release restores prior play state (Slip handled by the engine). Bend zone and mouse wheel: **not wired in A** (no safe API, see B); the zone is painted as a subtle band and a drag there is ignored. Scroll wheel over platter does nothing in A.
- Mode: a 4th tab labelled "Platter" rendered by `deck_view.rs` with its own `thread_local` per-deck `Cell<bool>`; `tab_rects` (shared) gives 3 cells, so in A the Platter toggle is a small round button in the pitch column header instead. Choose one in code review; both avoid editing `om-ui`.
- Tests: unit tests of Task 6 plus a headless test that `platter_geometry` fits inside a 260 px tall card without overlapping `l.transport`.

### Task 8: visual verification
1. `tools/screenshots.sh main-dark main-daylight main-stems`, view each PNG. Check: Play/Cue round and 56 px, Play shows Pause while playing (use `--demo-automix` scene `main-automix`), lit toggles use the deck accent, empty pads dashed with "Set", set pads coloured with names, loop stepper readable, platter ring with progress arc and static label, no text under 12 px, no overlap at the screenshot window size (1280x800) or at a narrow window.
2. Daylight: confirm text contrast on lit pads and glow not blown out.
3. Compare to `before.png`; adjust spacing only in `deck_view.rs`.

### Task 9: perf check, four decks
- Run the app with four decks loaded and playing (`--mock-device`; load via the demo library), watch the existing frame report from `frame_stats.rs` (`REPORT_EVERY` 5 s: p50/p95/p99). Budget: platter adds < 0.3 ms per deck to `build` p95, total < 1.2 ms for four decks; no regression in p99 vs a run on `main`.
- Add a debug-only counter (in `platter_paint.rs`, behind `cfg(debug_assertions)`) that logs path vertex counts per frame; assert < 400 per platter in a unit test of the builder.
- If over budget: lower arc segment count, skip groove rebuilds when the size is unchanged, draw ticks as one path.
- Only now commit regenerated screenshots (`docs/screenshots/*.png`, they are already modified; commit only the ones this plan regenerated).

### Task 10: shared-file edits (last, minimal, optional)
Only if the prior tasks show a need. Each is its own small commit, coordinate with the other plans:
1. `crates/om-ui-gpui/src/lib.rs`: add `pub mod platter_paint;` (one line; required for Task 7, so do it at the START of Task 6 if the build needs it, it is a one-line conflict at worst).
2. `crates/om-ui/src/widgets/transport.rs`: label text ("Cue", "Sync", "Tempo leader", "Snap to beat") and tooltips; update `tests/deck_widgets.rs` assertions.
3. `crates/om-ui/src/widgets/deck.rs`: `PadMode::Platter` plus `tab_rects` for 4 cells and layout metrics for round buttons (replace local constants).
4. `crates/om-ui/src/theme/tokens.rs`: move the proposed dark/daylight values (card, well, border, lit text) into tokens; spec says tokens, not hard-coded.

## Part B: model and engine additions (OPTIONAL, separate PRs, not required for A)

Each item is independent, test-first in its own crate, and A degrades gracefully without it.

1. **Loop in / Loop out.** `DeckParam::LoopIn`, `DeckParam::LoopOut` (om-core `param_id` parse/Display tests, doc comment line 7-9), handled in `om-engine/src/deck_control.rs::set_deck_param`: In stores a pending start (quantized via `quantizer` when `quantize`), Out calls `Deck::set_loop` with `start..position`. Snapshot already exposes `loop_start/loop_end`. Then UI adds two pill buttons (Task 5 hook).
2. **Beat jump.** `DeckParam::BeatJump(f64)` (signed beats), implemented in `DeckControl` using the `BeatGrid` and `Deck::seek`; keeps slip in sync. UI: back/forward stepper with size chip.
3. **Scrub / bend API.** `DeckParam::ScrubStart/ScrubTo(frame)/ScrubEnd` (engine pauses the motor, follows the target through a smoother on the audio side, resumes) and `DeckParam::Nudge(f64)` as a temporary additive rate offset that does not change the stored `Rate` (so the pitch fader is unaffected). Needs an audio-thread-safe atomic and a no-alloc test (`alloc_guard.rs`). Wire Bend zone and wheel (20 ms per notch) to Nudge.
4. **Cue names and colours.** Persist in `om-library` `cue_dao.rs` (name, colour index); expose through `HotcuePads` as a UI-side model (`om-ui`), not the audio snapshot (strings do not belong in `DeckSnapshot`, which is `Copy`). UI: right-click rename/recolour.
5. **Artwork.** Needs decode; only with an approved dependency (Cargo.lock change). Out of scope.

## Risks

1. Platter space: controls column is ~230 px; a 160 px platter does not fit with transport + pads. Mitigated by compact 112 px, a Platter toggle, and not editing shared layout until Task 10.
2. Scratch through `DeckParam::Seek` has control-timer latency and may click; A ships scratch as best-effort and documents it; B3 is the real fix. If it feels bad, ship the platter as display-only (progress ring and marker) and hold scratch for B.
3. Per-frame path rebuild x 4 decks at 60 fps: cached static geometry, Task 9 budget.
4. Round buttons below the 44 px touch floor in compact: keep 44 minimum (spec says 56; fall back to 44).
5. Glow/contrast on daylight; the contrast test in Task 1 gates it.
6. Merge conflicts: other redesign plans touch `app.rs`, `mixer_view.rs`, theme tokens; this plan stays inside the two owned files until Task 10.
7. Renaming Master to Tempo leader: keep "master" in tooltip.
8. Never `git add -A`: `Cargo.lock`, `docs/design/research/`, `tools/graphify_rebuild.py` are other work.
