# Tauri Front End: Foundation and Decks Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run OpenMixer's decks screen (waveforms, transport, mixer, meters) in a Tauri v2 window on the real engine, beside the GPUI window, and measure it against GPUI so the migration can be accepted or dropped.

**Architecture:** A toolkit-free crate `om-ui-bridge` turns engine state into wire formats (binary snapshot packet, packed waveform peaks, theme CSS) and turns control messages into `ParamId` dispatches. A thin shell crate `om-ui-tauri` owns the window, the Tauri commands and a 30 Hz control-side stream thread. A Svelte front end draws the waveform with one WebGL2 shader and everything else with DOM and CSS transforms. GPUI stays the default; the new window is behind the `gui-tauri` feature and the `--ui=tauri` flag.

**Tech Stack:** Rust 1.95, Tauri 2 (`tauri = "2"`, `tauri-build = "2"`), Svelte 5, TypeScript 5, Vite, Vitest, WebGL2.

**Spec:** There is no separate spec document. This plan implements the conclusions of the Tauri spike (source in `openmixer-tauri-spike/` beside this repo; results in `C:\Users\micro\openmixer-tauri-spike\results\`). Task 1 writes those conclusions into `docs/adr/0009-web-front-end-tauri-trial.md`, which is the spec from then on.

## Scope

This plan is the first of four. It ends at a decision gate (Task 11). The later plans are written only if the gate passes:

1. **This plan:** bridge, shell, decks slice, measurement.
2. Library (virtualised table, search, drag to deck), sampler, FX, stems, automix bar, loudness.
3. Settings pages, mapping editor, plugin windows, media keys, OS file drop, shortcuts.
4. Test tooling parity (gallery, end-to-end, screenshots), cut-over, deletion of `om-ui-gpui`, installers.

## Global Constraints

- PRs only. Never push or merge to `main`. Branch from `main`, `gh pr create --base main`, merge commits (no squash). PR description has Summary, Changes, Testing, Follow-ups and states whether it touches an extension point.
- Audio thread: no allocation, locks, blocking calls, I/O or logging. Nothing in this plan runs on the audio thread. The stream thread reads `DjEngine::snapshot()` only.
- No string ids parsed on the audio thread. Control ids are parsed with `ParamId::from_str` in the Tauri command handler, on a control thread.
- Plugin targets: `set_parameter` must never block. A Tauri command must not wait on a plugin.
- Rust: no `unsafe` in the new crates (`#![forbid(unsafe_code)]`); no `unwrap`/`expect` on IPC, file or user input; `thiserror` in library crates, `AppError` in the binary; `tracing` for logs.
- Code style: one-line comments that say why; functions about 10 lines; one responsibility per module.
- UI: colours, sizes and fonts come from theme tokens (CSS variables generated in Task 5), never literals in component CSS. Numbers use the numeral font (`.num` class, Ubuntu Mono).
- A UI change needs a screenshot that has been looked at, taken with `import -silent` into a temp dir, at most 6 per task.
- Never play audio. Every run in this plan uses `--mock-device`.
- A new `--demo-*` flag goes in `openmixer::flags::DEMO_FLAGS` and never writes the user's files. This plan adds none.
- WSL: one cargo process at a time, never in the background, never clippy and test in one command; `flock /tmp/openmixer-cargo.lock` when sharing a checkout. GUI runs need `WAYLAND_DISPLAY=`.
- Test command form: `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p <crate>`.
- Once per PR: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo deny check`, then `cargo run -p openmixer -- --smoke`.
- Workflow changes need a local `act` run.
- Wire formats are little-endian.
- Default snapshot stream rate is 30 Hz. Default frame cap is 60 fps.

## Review Focus

1. **A deck with no waveform yet** (track loaded, analysis still running, `WaveformData::levels()` empty): the lane shows the empty well and the transport still works. Pinned in Task 3 (`pack_peaks_empty_levels`) and Task 9 (`pickLevel` on an empty list).
2. **The webview reloads or closes while the stream runs**: the stream thread ends on the first failed send and does not panic or leak. Pinned in Task 7 (`stream_stops_when_sink_fails`).
3. **A control id the engine does not know, or a non-finite value**: rejected with an error, nothing dispatched, no panic. Pinned in Task 4 (`unknown_id_is_rejected`, `non_finite_value_is_rejected`).
4. **Position outside the track** (`position_frames` negative during pre-roll, or past `length_frames` in a loop): the playhead and time read-outs clamp; the shader draws the well outside the track. Pinned in Task 8 (`interpolate_clamps`).
5. **The window is resized to a sliver, or the display scale changes**: the canvas resizes without a GL error and no division by zero in pixels-per-point. Pinned in Task 9 (`viewScale_zero_width`).

---

## File Structure

```
crates/om-ui-bridge/                 toolkit-free; headless tests
  Cargo.toml
  src/lib.rs                         re-exports, BridgeError
  src/snapshot_packet.rs             EngineSnapshot -> bytes (Task 2)
  src/peaks.rs                       WaveformData -> packed RGB levels (Task 3)
  src/control.rs                     (id string, value) -> router dispatch (Task 4)
  src/theme_css.rs                   ThemeTokens -> CSS variables (Task 5)
crates/om-ui-tauri/                  the window; thin
  Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
  src/lib.rs                         run(), RunOptions
  src/commands.rs                    Tauri commands (Task 6, 7)
  src/stream.rs                      30 Hz stream loop (Task 7)
  web/                               Svelte front end
    package.json, vite.config.ts, tsconfig.json, index.html
    src/main.ts, src/App.svelte
    src/lib/ipc.ts                   packet decode, typed invoke wrappers (Task 8)
    src/lib/clock.ts                 playhead interpolation (Task 8)
    src/lib/waveform/shader.ts       GLSL source (Task 9)
    src/lib/waveform/view.ts         pickLevel, viewScale, barStep (Task 9)
    src/lib/waveform/Waveforms.svelte   one canvas, all lanes (Task 9)
    src/lib/controls/{Knob,Fader,Meter,Button}.svelte (Task 10)
    src/lib/deck/{DeckHeader,Overview,Transport}.svelte (Task 10)
    src/lib/mixer/{ChannelStrip,Mixer}.svelte (Task 10)
    src/lib/bench.ts                 frame statistics (Task 11)
crates/openmixer/src/gui_tauri.rs    composition for the Tauri window (Task 6)
fixtures/ui-bridge/snapshot-v1.bin   golden packet shared by Rust and TS tests (Task 2)
docs/adr/0009-web-front-end-tauri-trial.md (Task 1)
tools/ui-bench.sh                    runs either window and samples CPU (Task 11)
```

---

### Task 1: Record the decision to trial

**Files:**
- Create: `docs/adr/0009-web-front-end-tauri-trial.md`
- Modify: `docs/adr/INDEX.md`, `docs/adr/SUMMARY.md`, `docs/DESIGN_PATTERNS.md`

**Interfaces:**
- Produces: the spec the remaining tasks cite. Status `Proposed (trial)` until Task 11 changes it.

- [ ] **Step 1: Write ADR 0009.** Sections: Context, Decision, Evidence, Consequences, Exit criteria. Content that must appear:
  - Context: GPUI needs hand-rolled controls and layout (`om-ui-gpui` paint modules, `om-ui` geometry modules); ADR 0004's 2026-11-07 update puts the app chrome on `gpui-component` while knobs, faders, meters and waveforms stay custom-painted.
  - Decision: trial a Tauri v2 window beside GPUI behind `gui-tauri`. ADR 0004's UI choice stands until the trial ends. ADR 0008 (the look) is unchanged and is carried by CSS variables generated from the theme files.
  - Evidence, Windows, 1600×780, two decks playing, CPU as percent of one core summed over the app and its WebView2 processes: 43% at 120 fps; 26% capped to 60 fps; 17% capped to 30 fps; 13% paused; 35% with a 30 Hz stream against 49% at 120 Hz; 4 KB packets add 6 to 11 points; no frame over 20 ms except one 31 ms frame in the 120 Hz stream run; zero packets lost.
  - Evidence, limits: frame figures count animation callbacks, not presented frames; GPU load not measured; no GPUI baseline; nothing larger than 1600×780; Linux measured only under WSL software rendering, where `WEBKIT_DISABLE_DMABUF_RENDERER=1` cut CPU from 213% to 56%.
  - Rendering rules adopted: analyse once and upload peaks once as a texture; one shader draws all lanes; meters, playheads, bar numbers and cue flags move by CSS transform; skip the frame when nothing changed; write text only when it changes; 30 Hz stream with interpolation; small per-frame packets, bulk data once.
  - Rejected: pre-rendered tile scrolling (must rebuild on zoom), batched canvas paths (1.4 GB of memory and dropped frames).
  - Exit criteria: copy the thresholds from Task 11 Step 5.
- [ ] **Step 2: Add the ADR to `docs/adr/INDEX.md` and a five-line summary to `docs/adr/SUMMARY.md`.**
- [ ] **Step 3: Add a "Front-end bridge" entry to `docs/DESIGN_PATTERNS.md`** naming the two crates, the rule that `om-ui-bridge` has no window dependency, and the one-way data flow (snapshot out, `ParamId` dispatch in).
- [ ] **Step 4: Commit and open the PR.**

```bash
git checkout -b docs/adr-0009-tauri-trial main
git add docs/adr docs/DESIGN_PATTERNS.md
git commit -m "docs: ADR 0009, trial a Tauri front end beside GPUI"
gh pr create --base main
```

---

### Task 2: Snapshot packet

**Files:**
- Create: `crates/om-ui-bridge/Cargo.toml`, `crates/om-ui-bridge/src/lib.rs`, `crates/om-ui-bridge/src/snapshot_packet.rs`, `fixtures/ui-bridge/snapshot-v1.bin`
- Modify: `Cargo.toml` (add `"crates/om-ui-bridge"` to `members`; add `om-ui-bridge = { path = "crates/om-ui-bridge" }` to `[workspace.dependencies]`)

**Interfaces:**
- Consumes: `om_core::EngineSnapshot` (`decks: Vec<DeckSnapshot>`, `mixer: MixerSnapshot`, `ticks: u64`).
- Produces:
  - `pub const PACKET_MAGIC: u32 = 0x4F4D_5331;` (`OMS1`), `pub const PACKET_VERSION: u16 = 1;`
  - `pub fn pack_snapshot(snap: &EngineSnapshot, age_ms: f32, out: &mut Vec<u8>)` — clears and fills `out`.
  - `pub fn packet_len(decks: usize, channels: usize) -> usize` — `24 + decks * 40 + channels * 32 + 16`.
  - `#[derive(thiserror::Error)] pub enum BridgeError` in `lib.rs` with variants `UnknownControl(String)`, `NonFinite`, `Rejected(String)`.

Packet layout, little-endian:

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | magic |
| 4 | u16 | version |
| 6 | u8 | deck count |
| 7 | u8 | channel count |
| 8 | u64 | `ticks` |
| 16 | f32 | `age_ms` (time from snapshot pull to send) |
| 20 | u32 | reserved, zero |
| 24 + 40·d | i64, i64, f64, f64, u8, 7 pad | per deck: `position_frames`, `length_frames`, `rate`, `file_sample_rate`, flags |
| after decks, + 32·c | 8 × f32 | per channel: `peak_left`, `peak_right`, `fader`, `trim`, `filter`, `eq[0]`, `eq[1]`, `eq[2]` |
| last 16 | 4 × f32 | `master_peak_left`, `master_peak_right`, `master_gain`, `crossfader` |

Deck flags: bit 0 `loaded`, bit 1 `playing`, bit 2 `looping`, bit 3 `sync`, bit 4 `keylock`.

- [ ] **Step 1: Write the failing tests** in `snapshot_packet.rs`:

```rust
#[test]
fn packet_len_two_decks_two_channels() { assert_eq!(packet_len(2, 2), 184); }

#[test]
fn header_fields() {
    let mut out = Vec::new();
    pack_snapshot(&fixture(), 2.5, &mut out);
    assert_eq!(out.len(), 184);
    assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), PACKET_MAGIC);
    assert_eq!(u16::from_le_bytes(out[4..6].try_into().unwrap()), 1);
    assert_eq!((out[6], out[7]), (2, 2));
    assert_eq!(f32::from_le_bytes(out[16..20].try_into().unwrap()), 2.5);
}

#[test]
fn deck_block() {
    let mut out = Vec::new();
    pack_snapshot(&fixture(), 0.0, &mut out);
    assert_eq!(i64::from_le_bytes(out[24..32].try_into().unwrap()), 441_000);
    assert_eq!(i64::from_le_bytes(out[32..40].try_into().unwrap()), 11_025_000);
    assert_eq!(f64::from_le_bytes(out[40..48].try_into().unwrap()), 1.02);
    assert_eq!(out[56], 0b0000_0011); // loaded and playing
}

#[test]
fn channel_and_master_blocks() {
    let mut out = Vec::new();
    pack_snapshot(&fixture(), 0.0, &mut out);
    let f = |o: usize| f32::from_le_bytes(out[o..o + 4].try_into().unwrap());
    assert_eq!((f(104), f(108), f(112)), (0.5, 0.25, 0.85)); // channel 0: peaks, fader
    assert_eq!((f(168), f(172), f(180)), (0.75, 0.6, 0.5)); // master peaks, crossfader
}

#[test]
fn reuses_the_buffer() {
    let mut out = vec![9u8; 500];
    pack_snapshot(&fixture(), 0.0, &mut out);
    assert_eq!(out.len(), 184);
}

#[test]
fn matches_golden_file() {
    let mut out = Vec::new();
    pack_snapshot(&fixture(), 2.5, &mut out);
    assert_eq!(out, include_bytes!("../../../fixtures/ui-bridge/snapshot-v1.bin"));
}
```

`fixture()` builds an `EngineSnapshot` (use `Default` where the type has it) with two decks and two channels: deck 0 `loaded`, `playing`, `position_frames = 441_000`, `length_frames = 11_025_000`, `rate = 1.02`, `file_sample_rate = 44_100.0`; deck 1 all defaults; channel 0 peaks `0.5, 0.25` and `fader = 0.85`; master peaks `0.75, 0.6`; `crossfader = 0.5`; `ticks = 7`.

- [ ] **Step 2: Run to verify failure.** `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-ui-bridge` — expected: does not compile, `pack_snapshot` not found.
- [ ] **Step 3: Implement `packet_len` and `pack_snapshot`.** Crate dependencies: `om-core`, `om-analysis`, `om-ui`, `thiserror`. `#![forbid(unsafe_code)]`.
- [ ] **Step 4: Write the golden file.** Add an `#[ignore]` test `write_golden` that writes `fixtures/ui-bridge/snapshot-v1.bin`; run it once with `cargo test -p om-ui-bridge write_golden -- --ignored`.
- [ ] **Step 5: Run the tests.** Expected: 6 passed, 1 ignored.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui-bridge): binary snapshot packet"`

---

### Task 3: Waveform peaks

**Files:**
- Create: `crates/om-ui-bridge/src/peaks.rs`

**Interfaces:**
- Consumes: `om_analysis::WaveformData` (`levels() -> &[WaveformLevel]`, `sample_rate()`, `num_samples()`), `WaveformLevel { samples_per_point: usize, points: Vec<WaveformPoint> }`, `WaveformPoint { low, mid, high }`, `om_ui::waveform::bands::compress(f32) -> f32`.
- Produces:
  - `pub struct PeakLevel { pub samples_per_point: u32, pub len: u32 }`
  - `pub struct PeakBlob { pub sample_rate: f64, pub num_samples: i64, pub levels: Vec<PeakLevel>, pub bytes: Vec<u8> }`
  - `pub fn pack_peaks(data: &WaveformData) -> PeakBlob` — `bytes` is every level in order, three bytes per point: low, mid, high, each `(compress(v).clamp(0.0, 1.0) * 255.0).round() as u8`.

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn pack_peaks_layout() {
    let data = two_level_fixture(); // level 0: 4 points at 256 samples; level 1: 2 points at 512
    let blob = pack_peaks(&data);
    assert_eq!(blob.levels.len(), 2);
    assert_eq!((blob.levels[0].samples_per_point, blob.levels[0].len), (256, 4));
    assert_eq!((blob.levels[1].samples_per_point, blob.levels[1].len), (512, 2));
    assert_eq!(blob.bytes.len(), (4 + 2) * 3);
}

#[test]
fn pack_peaks_uses_band_compression() {
    let blob = pack_peaks(&two_level_fixture()); // point 0 has low = 1.0, mid = 0.25, high = 0.0
    assert_eq!(blob.bytes[0], 255);
    assert_eq!(blob.bytes[1], (compress(0.25) * 255.0).round() as u8);
    assert_eq!(blob.bytes[2], 0);
}

#[test]
fn pack_peaks_clamps_out_of_range() {
    let blob = pack_peaks(&fixture_with_low(3.0));
    assert_eq!(blob.bytes[0], 255);
}

#[test]
fn pack_peaks_empty_levels() {
    let blob = pack_peaks(&WaveformData::default());
    assert!(blob.levels.is_empty() && blob.bytes.is_empty());
}
```

- [ ] **Step 2: Run to verify failure.** Expected: `pack_peaks` not found.
- [ ] **Step 3: Implement `pack_peaks`.**
- [ ] **Step 4: Run the tests.** Expected: 4 passed.
- [ ] **Step 5: Commit.** `git commit -m "feat(ui-bridge): pack waveform levels for the GPU"`

---

### Task 4: Control dispatch

**Files:**
- Create: `crates/om-ui-bridge/src/control.rs`

**Interfaces:**
- Consumes: `om_core::ParamId` (`FromStr`, `validate()`), `om_core::ParameterRouter::dispatch(&self, id: &ParamId, value: f64) -> bool`, `BridgeError` from Task 2.
- Produces: `pub fn dispatch_control(router: &ParameterRouter, id: &str, value: f64) -> Result<(), BridgeError>`
  - `value` not finite → `BridgeError::NonFinite`, nothing dispatched.
  - `id` fails `ParamId::from_str` or `validate()` → `BridgeError::UnknownControl(id.to_owned())`.
  - `dispatch` returns `false` → `BridgeError::Rejected(id.to_owned())`.

Read `docs/guidelines/control-ids.md` and `crates/om-ui/src/binding.rs` before writing the tests. Ids are built from `ParamId` values and printed with `Display`; no id string is typed by hand, in Rust or in the front end.

Also produce `pub fn slice_control_ids(decks: u8, channels: u8) -> Vec<(String, String)>`: pairs of a stable front-end key and the canonical id, for every control the decks slice uses. Keys: `deck{N}.play`, `deck{N}.cue`, `deck{N}.sync`, `deck{N}.tempo`, `deck{N}.seek`, `ch{N}.fader`, `ch{N}.trim`, `ch{N}.filter`, `ch{N}.eq.low`, `ch{N}.eq.mid`, `ch{N}.eq.high`, `mixer.crossfader`, `mixer.master`, with `N` from 0. Where the engine has no matching `ParamId` variant, leave the key out and list it in the task report. Add the test `slice_ids_round_trip`: every id parses back with `ParamId::from_str` to a value whose `Display` equals the id, and keys are unique.

- [ ] **Step 1: Write the failing tests** using a `ParameterRouter` with one registered recording target (follow the pattern of the router's own tests in `crates/om-core/src/router.rs`):

```rust
#[test]
fn known_id_reaches_the_target() {
    let (router, seen) = router_with_recorder();
    dispatch_control(&router, DECK0_PLAY, 1.0).unwrap();
    assert_eq!(seen.lock().unwrap().as_slice(), &[(DECK0_PLAY.to_owned(), 1.0)]);
}

#[test]
fn unknown_id_is_rejected() {
    let (router, seen) = router_with_recorder();
    assert!(matches!(dispatch_control(&router, "not.a.real.id", 1.0), Err(BridgeError::UnknownControl(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn non_finite_value_is_rejected() {
    let (router, seen) = router_with_recorder();
    assert!(matches!(dispatch_control(&router, DECK0_PLAY, f64::NAN), Err(BridgeError::NonFinite)));
    assert!(matches!(dispatch_control(&router, DECK0_PLAY, f64::INFINITY), Err(BridgeError::NonFinite)));
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn unregistered_namespace_is_rejected() {
    let router = ParameterRouter::new();
    assert!(matches!(dispatch_control(&router, DECK0_PLAY, 1.0), Err(BridgeError::Rejected(_))));
}
```

`DECK0_PLAY` is the id for key `deck0.play` from `slice_control_ids(2, 2)`.

- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement `dispatch_control`.**
- [ ] **Step 4: Run the tests.** Expected: 4 passed.
- [ ] **Step 5: Commit.** `git commit -m "feat(ui-bridge): validated control dispatch"`

---

### Task 5: Theme as CSS variables

**Files:**
- Create: `crates/om-ui-bridge/src/theme_css.rs`

**Interfaces:**
- Consumes: `om_ui::theme::ThemeTokens` (see `crates/om-ui/src/theme/tokens.rs`), `ThemeRegistry::load_builtins()`, `ThemeRegistry::select(id)`, `ThemeRegistry::current()`.
- Produces: `pub fn theme_css(tokens: &ThemeTokens) -> String` — one `:root { ... }` block. `ThemeTokens` is a flat struct; each variable is `--om-` plus the Rust field name in kebab-case:
  - `Colour` fields: `--om-ground`, `--om-surface-raised`, `--om-text-primary`, `--om-playhead`, … as `#rrggbb`, or `rgb(r g b / a)` when alpha is below 1.
  - `accents`: `--om-accent-0` … `--om-accent-N`.
  - Lengths (gaps, radii, type sizes, heights, widths): `px` values, e.g. `--om-gap: 13px`, `--om-type-numeral-large: 21px`, `--om-mixer-meter-width: 12px`.
  - Unitless fields (names ending `_alpha`, `_db`, `_seconds`, `_level`, and `type_scale`): plain numbers, e.g. `--om-grid-bar-alpha: 0.14`.
  - Fields that are neither a colour nor a number (for example waveform colour stops) are skipped in this task.
  - Output is sorted by variable name so it is stable.

- [ ] **Step 1: Write the failing tests** against the built-in `openmixer.dark` theme:

```rust
#[test]
fn dark_theme_colours() {
    let css = theme_css(dark());
    assert!(css.starts_with(":root {"));
    assert!(css.contains("--om-ground: #121211;"));
    assert!(css.contains("--om-playhead: #ef5a3f;"));
    assert!(css.contains("--om-accent-0: #e3a24f;"));
    assert!(css.contains("--om-accent-1: #86a9cb;"));
}

#[test]
fn translucent_colour_keeps_alpha() {
    assert!(theme_css(dark()).contains("--om-shadow: rgb(0 0 0 / 0.5"));
}

#[test]
fn lengths_and_numbers() {
    let css = theme_css(dark());
    assert!(css.contains("--om-gap: 13px;"));
    assert!(css.contains("--om-mixer-meter-width: 12px;"));
    assert!(css.contains("--om-grid-bar-alpha: 0.14;"));
}

#[test]
fn output_is_sorted_and_stable() {
    let css = theme_css(dark());
    let names: Vec<&str> = css.lines().filter_map(|l| l.trim().split(':').next()).filter(|n| n.starts_with("--om-")).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert_eq!(css, theme_css(dark()));
}
```

- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement `theme_css`.** Use the active density's metric set (`ThemeTokens::metrics(Density)`); take the density as a second parameter if `tokens.rs` requires it and update the tests' helper.
- [ ] **Step 4: Run the tests.** Expected: 4 passed.
- [ ] **Step 5: Run the per-PR checks and open the PR for Tasks 2 to 5.**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo deny check
nice -n 19 env CARGO_BUILD_JOBS=3 cargo test --workspace
git commit -m "feat(ui-bridge): theme tokens as CSS variables"
gh pr create --base main
```

---

### Task 6: The Tauri shell opens a themed, empty window

**Files:**
- Create: `crates/om-ui-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,icons/icon.png,icons/icon.ico}`, `crates/om-ui-tauri/src/{lib.rs,commands.rs}`, `crates/om-ui-tauri/web/{package.json,vite.config.ts,tsconfig.json,index.html,src/main.ts,src/App.svelte}`, `crates/openmixer/src/gui_tauri.rs`
- Modify: `Cargo.toml` (member and workspace dependency `om-ui-tauri`), `crates/openmixer/Cargo.toml` (optional dependency; feature `gui-tauri = ["dep:om-ui-tauri"]`, not in `default`), `crates/openmixer/src/main.rs`, `.github/workflows/rust.yml`, `docs/guidelines/build.md`, `README.md`, `deny.toml` if `cargo deny check` reports new licences

**Interfaces:**
- Consumes: `theme_css` (Task 5), `AppComposition` (`engine: Arc<DjEngine>`, `router: Arc<ParameterRouter>`, `themes: ThemeRegistry`).
- Produces:
  - `om_ui_tauri::RunOptions { pub smoke: bool, pub banner: Option<String>, pub bench_seconds: Option<u32> }`
  - `om_ui_tauri::run(engine: Arc<DjEngine>, router: Arc<ParameterRouter>, theme_css: String, options: RunOptions) -> Result<(), om_ui_tauri::ShellError>`
  - Tauri command `theme_css() -> String`.
  - Tauri command `ready()` — the front end calls it after first paint; with `smoke` set, the app exits with code 0.
  - `openmixer::gui_tauri::main(args: &[String]) -> ExitCode` (compiled under `#[cfg(feature = "gui-tauri")]`).
  - `main.rs`: when the feature is on and the arguments contain `--ui=tauri`, remove that argument and call `gui_tauri::main`.

Decisions:
- `tauri.conf.json`: `identifier` `org.openmixer.app`, `productName` `OpenMixer`, window `1600 × 780`, title `OpenMixer`, `frontendDist` `./web/dist`, `withGlobalTauri` `false`, `bundle.active` `false`.
- `build.rs` creates `web/dist/index.html` containing `<!doctype html><title>OpenMixer</title>` when the file is missing, then calls `tauri_build::build()`. This keeps `cargo check --workspace` working without Node.
- On Linux, `run` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` before building the app unless the variable is already set (ADR 0009 evidence).
- `gui_tauri::main` reuses the composition steps of `crates/openmixer/src/gui.rs:47` for the engine, router, mock device, demo flags and theme selection. It does not copy the GPUI-only parts (settings overlay, debug panel, library hub). Move shared steps into a function in `composition.rs` instead of duplicating them.
- Front end: `App.svelte` calls `theme_css`, injects it in a `<style>` element, renders the four layout regions (top bar, deck headers, waveforms, controls) as empty blocks using only `--om-*` variables, then calls `ready`. Fonts are loaded with `@font-face` from `resources/fonts/`, copied into `web/public/fonts/` by a `prebuild` npm script.
- `package.json` scripts: `dev`, `build`, `test` (Vitest), `check` (`svelte-check`). Pin exact versions and commit `package-lock.json`. Use Node 22 LTS.

- [ ] **Step 1: Write the failing Rust test** in `crates/openmixer/tests/` (follow an existing test there for the invocation pattern):

```rust
#[test]
#[cfg(feature = "gui-tauri")]
fn tauri_window_smoke() {
    let out = run_openmixer(&["--ui=tauri", "--mock-device", "--smoke"]); // WAYLAND_DISPLAY unset in the helper
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}
```

- [ ] **Step 2: Run to verify failure.** `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p openmixer --features gui-tauri tauri_window_smoke` — expected: fails, unknown feature.
- [ ] **Step 3: Scaffold the web project and build it.** `cd crates/om-ui-tauri/web && npm ci && npm run build` — expected: `dist/index.html` exists.
- [ ] **Step 4: Implement the shell crate, the feature, `gui_tauri.rs` and the `main.rs` dispatch.**
- [ ] **Step 5: Run the test.** Expected: PASS.
- [ ] **Step 6: Confirm GPUI is untouched.** `cargo run -p openmixer -- --smoke` — expected: `smoke ok`.
- [ ] **Step 7: Take one screenshot** of `WAYLAND_DISPLAY= cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device` with `import -silent -window <id>` and look at it: four regions, dark faceplate colours, bundled fonts.
- [ ] **Step 8: Add CI.** In `.github/workflows/rust.yml` add a job that installs `libwebkit2gtk-4.1-dev`, sets up Node 22, runs `npm ci && npm run build && npm test` in `crates/om-ui-tauri/web`, then `cargo test -p openmixer --features gui-tauri` under `xvfb-run`. Run it locally with `act`.
- [ ] **Step 9: Update `docs/guidelines/build.md` and `README.md`** with the new Linux package, the Node requirement and the two commands.
- [ ] **Step 10: Per-PR checks, commit, PR.** `git commit -m "feat(ui-tauri): themed empty window behind gui-tauri"`

---

### Task 7: Stream and commands

**Files:**
- Create: `crates/om-ui-tauri/src/stream.rs`
- Modify: `crates/om-ui-tauri/src/{lib.rs,commands.rs}`, `crates/openmixer/src/gui_tauri.rs`

**Interfaces:**
- Consumes: `pack_snapshot`, `pack_peaks`, `dispatch_control` (Tasks 2 to 4), `om_ui::library::TrackLoader` (`pump`, `waveform(deck) -> Option<&Arc<WaveformData>>`, `track_info(deck) -> TrackInfo`), `DjEngine::snapshot()`.
- Produces:
  - `pub trait PacketSink: Send { fn send(&mut self, bytes: &[u8]) -> bool; }` — `false` means the receiver is gone.
  - `pub struct StreamConfig { pub hz: f64 }` with `Default` giving `30.0`.
  - `pub fn run_stream(engine: Arc<DjEngine>, sink: impl PacketSink, config: StreamConfig, stop: Arc<AtomicBool>)` — loops until `stop` is set or `sink.send` returns `false`; one `Vec<u8>` reused for every packet.
  - Tauri commands:
    - `start_stream(channel: Channel<InvokeResponseBody>)` — spawns `run_stream` on a named thread `om-ui-stream`; a second call stops the first stream.
    - `control(id: String, value: f64) -> Result<(), String>` — `dispatch_control`.
    - `deck_peaks(deck: usize) -> tauri::ipc::Response` — `PeakBlob.bytes`, empty when the deck has no waveform.
    - `deck_info(deck: usize) -> DeckInfo` — `#[derive(Serialize)] struct DeckInfo { title: String, artist: String, bpm: Option<f64>, key: Option<String>, sample_rate: f64, num_samples: i64, levels: Vec<PeakLevelDto>, hotcues: Vec<Option<i64>>, generation: u64 }`, camelCase on the wire.
  - Tauri event `deck-changed` with payload `{ deck: usize, generation: u64 }`, emitted when a deck's waveform `Arc` pointer or hot cues change. `generation` increases by one per change.
  - The control loop that calls `TrackLoader::pump` runs on its own thread at 30 Hz and owns the loader behind a `Mutex` shared with `deck_peaks` and `deck_info`.

- [ ] **Step 1: Write the failing tests** in `stream.rs` using an engine from `om-testing` and a recording sink:

```rust
#[test]
fn stream_sends_valid_packets() {
    let (sink, seen) = recording_sink();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = spawn_stream(test_engine(), sink, StreamConfig { hz: 200.0 }, stop.clone());
    wait_until(|| seen.lock().unwrap().len() >= 3);
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    let first = &seen.lock().unwrap()[0];
    assert_eq!(first.len(), packet_len(2, 2));
    assert_eq!(u32::from_le_bytes(first[0..4].try_into().unwrap()), PACKET_MAGIC);
}

#[test]
fn stream_stops_when_sink_fails() {
    let handle = spawn_stream(test_engine(), FailingSink, StreamConfig::default(), Arc::new(AtomicBool::new(false)));
    handle.join().unwrap(); // returns instead of hanging
}

#[test]
fn stream_rate_is_respected() {
    let (sink, seen) = recording_sink();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = spawn_stream(test_engine(), sink, StreamConfig { hz: 30.0 }, stop.clone());
    std::thread::sleep(Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    let n = seen.lock().unwrap().len();
    assert!((12..=18).contains(&n), "{n} packets in 500 ms at 30 Hz");
}
```

- [ ] **Step 2: Run to verify failure.** `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-ui-tauri`
- [ ] **Step 3: Implement `run_stream`**, pacing with a deadline that is advanced by one period per packet and reset when it falls behind.
- [ ] **Step 4: Implement the commands, the `deck-changed` event and the loader thread.**
- [ ] **Step 5: Run the tests.** Expected: 3 passed.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui-tauri): snapshot stream, control and deck data commands"`

---

### Task 8: Front-end data layer

**Files:**
- Create: `crates/om-ui-tauri/web/src/lib/ipc.ts`, `web/src/lib/clock.ts`, `web/src/lib/ipc.test.ts`, `web/src/lib/clock.test.ts`

**Interfaces:**
- Consumes: the packet layout (Task 2), `fixtures/ui-bridge/snapshot-v1.bin`, the commands and event of Task 7.
- Produces:
  - `interface DeckFrame { position: number; length: number; rate: number; fileRate: number; loaded: boolean; playing: boolean; looping: boolean; sync: boolean; keylock: boolean }`
  - `interface Snapshot { ticks: bigint; ageMs: number; receivedAt: number; decks: DeckFrame[]; channels: ChannelFrame[]; master: { left: number; right: number; gain: number; crossfader: number } }`
  - `interface ChannelFrame { left: number; right: number; fader: number; trim: number; filter: number; eq: [number, number, number] }`
  - `decodeSnapshot(buf: ArrayBuffer, receivedAt: number): Snapshot` — throws `Error('bad packet')` on wrong magic, wrong version or short buffer.
  - `interpolate(deck: DeckFrame, snap: Snapshot, now: number): number` — frames; `position + (ageMs + now - receivedAt) / 1000 * fileRate * rate` when playing, else `position`; clamped to `[0, max(0, length - 1)]`.
  - `startStream(onSnapshot: (s: Snapshot) => void): Promise<void>`, `control(id: string, value: number): Promise<void>`, `deckInfo(deck: number)`, `deckPeaks(deck: number): Promise<Uint8Array>`, `onDeckChanged(cb)`.

- [ ] **Step 1: Write the failing tests:**

```ts
import { readFileSync } from 'node:fs';
const golden = readFileSync('../../../fixtures/ui-bridge/snapshot-v1.bin');
const buf = golden.buffer.slice(golden.byteOffset, golden.byteOffset + golden.byteLength);

test('decodes the golden packet', () => {
  const s = decodeSnapshot(buf, 1000);
  expect(s.decks).toHaveLength(2);
  expect(s.decks[0]).toMatchObject({ position: 441000, length: 11025000, rate: 1.02, fileRate: 44100, loaded: true, playing: true });
  expect(s.decks[1].loaded).toBe(false);
  expect(s.channels[0]).toMatchObject({ left: 0.5, right: 0.25 });
  expect(s.channels[0].fader).toBeCloseTo(0.85);
  expect(s.master.left).toBeCloseTo(0.75);
  expect(s.master.crossfader).toBe(0.5);
  expect(s.ageMs).toBe(2.5);
});

test('rejects a bad magic and a short buffer', () => {
  const bad = buf.slice(0); new DataView(bad).setUint32(0, 0, true);
  expect(() => decodeSnapshot(bad, 0)).toThrow('bad packet');
  expect(() => decodeSnapshot(buf.slice(0, 20), 0)).toThrow('bad packet');
});

test('interpolates a playing deck', () => {
  const s = decodeSnapshot(buf, 1000);
  expect(interpolate(s.decks[0], s, 1010)).toBeCloseTo(441000 + 12.5 / 1000 * 44100 * 1.02, 3);
});

test('interpolate_clamps', () => {
  const s = decodeSnapshot(buf, 0);
  expect(interpolate({ ...s.decks[0], position: -500 }, s, 0)).toBe(0);
  expect(interpolate({ ...s.decks[0], position: 11025000 }, s, 60000)).toBe(11024999);
  expect(interpolate({ ...s.decks[0], length: 0, position: 10 }, s, 0)).toBe(0);
});

test('a paused deck does not advance', () => {
  const s = decodeSnapshot(buf, 0);
  expect(interpolate({ ...s.decks[0], playing: false }, s, 5000)).toBe(441000);
});
```

- [ ] **Step 2: Run to verify failure.** `npm test` in `crates/om-ui-tauri/web` — expected: FAIL, modules not found.
- [ ] **Step 3: Implement `ipc.ts` and `clock.ts`.**
- [ ] **Step 4: Run the tests.** Expected: 5 passed.
- [ ] **Step 5: Commit.** `git commit -m "feat(ui-tauri): packet decode and playhead interpolation"`

---

### Task 9: Waveform component

**Files:**
- Create: `web/src/lib/waveform/{shader.ts,view.ts,view.test.ts,Waveforms.svelte}`
- Modify: `web/src/App.svelte`

**Interfaces:**
- Consumes: `Snapshot`, `interpolate`, `deckInfo`, `deckPeaks`, `onDeckChanged` (Task 8).
- Produces:
  - `pickLevel(levels: { samplesPerPoint: number; len: number }[], samplesPerPixel: number): number` — index of the coarsest level whose `samplesPerPoint <= samplesPerPixel`; `0` when none qualifies; `-1` for an empty list.
  - `viewScale(visibleSeconds: number, sampleRate: number, widthPx: number): number` — samples per device pixel; `0` when `widthPx <= 0`.
  - `barStep(pixelsPerBar: number, minPx = 46): number` — smallest power of two `n` with `pixelsPerBar * n >= minPx`.
  - `<Waveforms snapshot={...} visibleSeconds={8} fpsCap={60} />` — one canvas for all decks.

Rendering rules, from ADR 0009:
- One `RGB8` texture per deck, every level packed in rows 4096 texels wide; uploaded when `deck-changed` fires, never per frame.
- One fragment shader draws each lane. Port `FS` from the spike's `ui/app.js`, with two changes: the level index, its row offset, length and `samplesPerPoint` come in as uniforms chosen by `pickLevel`, and colours come from uniforms read from `--om-*` variables (`--om-well`, `--om-text-primary`, `--om-playhead`, `--om-accent-N`, `--om-grid-bar-alpha`, `--om-grid-beat-alpha`).
- The played side is dimmed by the factor `PLAYED_BRIGHTNESS` (`0.45`) from `om_ui::waveform::bands`; expose it through `deck_info` or a constant kept beside the shader with a comment naming the Rust source.
- Per frame: skip all GL and DOM work when no deck position, zoom or canvas size changed since the last frame.
- With `fpsCap` set, skip animation callbacks that arrive sooner than `1000 / fpsCap - 2` ms after the last drawn frame.
- Bar numbers and cue flags are pooled DOM elements moved with `translate3d`; text is written only when it changes.
- Wheel over the lanes zooms `visibleSeconds` between 1 and 64. Dragging a lane scrubs: send the deck's seek control id through `control`, and hold the local position until two snapshots arrive after the last seek.

- [ ] **Step 1: Write the failing tests** in `view.test.ts`:

```ts
const levels = [{ samplesPerPoint: 256, len: 44000 }, { samplesPerPoint: 1024, len: 11000 }, { samplesPerPoint: 4096, len: 2750 }];

test('pickLevel chooses the coarsest level that still resolves a pixel', () => {
  expect(pickLevel(levels, 100)).toBe(0);
  expect(pickLevel(levels, 256)).toBe(0);
  expect(pickLevel(levels, 2000)).toBe(1);
  expect(pickLevel(levels, 50000)).toBe(2);
});
test('pickLevel on an empty list', () => { expect(pickLevel([], 500)).toBe(-1); });

test('viewScale', () => { expect(viewScale(8, 44100, 1600)).toBeCloseTo(220.5); });
test('viewScale_zero_width', () => { expect(viewScale(8, 44100, 0)).toBe(0); expect(viewScale(8, 44100, -4)).toBe(0); });

test('barStep', () => {
  expect(barStep(120)).toBe(1);
  expect(barStep(46)).toBe(1);
  expect(barStep(30)).toBe(2);
  expect(barStep(5)).toBe(16);
});
```

- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement `view.ts`.**
- [ ] **Step 4: Run the tests.** Expected: 5 passed.
- [ ] **Step 5: Implement `shader.ts` and `Waveforms.svelte`.** A deck whose `pickLevel` is `-1` draws only the well and the playhead.
- [ ] **Step 6: Screenshot.** Run `WAYLAND_DISPLAY= cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device --demo-scene=cues`, capture the window with `import -silent`, and check as a DJ would: beats readable at 8 s, bar lines stronger than beat lines, playhead red and centred, played side dimmer, cue flags on their beats, deck hues correct.
- [ ] **Step 7: Commit.** `git commit -m "feat(ui-tauri): GPU waveform lanes"`

---

### Task 10: Deck and mixer components

**Files:**
- Create: `web/src/lib/controls/{Knob,Fader,Meter,Button}.svelte`, `web/src/lib/controls/drag.ts`, `web/src/lib/controls/drag.test.ts`, `web/src/lib/deck/{DeckHeader,Overview,Transport}.svelte`, `web/src/lib/mixer/{ChannelStrip,Mixer}.svelte`, `web/src/lib/format.ts`, `web/src/lib/format.test.ts`
- Modify: `web/src/App.svelte`

**Interfaces:**
- Consumes: `control`, `Snapshot`, `interpolate`, `deckInfo` (Task 8); the canonical control ids in `docs/guidelines/control-ids.md` for play, cue, sync, tempo, channel fader, trim, three EQ bands, filter, crossfader and master gain.
- Produces:
  - `knobDelta(value: number, dyPx: number, rangePx = 160): number` — `clamp(value - dyPx / rangePx, 0, 1)`.
  - `faderValue(pointerPx: number, trackStartPx: number, trackLengthPx: number, inverted: boolean): number` — clamped to `[0, 1]`; `0` when `trackLengthPx <= 0`.
  - `formatTime(seconds: number): string` — `mm:ss.t`, e.g. `04:17.3`; negative input formats its absolute value.
  - `formatBpm(bpm: number): string` — two decimals.
  - `formatPitch(rate: number): string` — signed percent with two decimals, e.g. `+2.00%`, `-0.50%`, `+0.00%`.

Decisions:
- Every knob, fader and button sends through `control(id, value)`; it never changes engine state any other way. Displayed values come from the snapshot's `ChannelFrame` and `master` fields (Task 8).
- While a control is being dragged it shows the local value; on release it follows the snapshot again.
- Meters are one element scaled with `scaleY`; they fall at most `0.03` per frame and update only when the shown value moves by more than `0.003`.
- Component CSS uses only `--om-*` variables. Deck hue appears only on elements that name a deck (letter chip, play button when on, set cue pads).
- Double-click resets a knob or fader to its default.

- [ ] **Step 1: Write the failing tests:**

```ts
test('knobDelta', () => {
  expect(knobDelta(0.5, -16)).toBeCloseTo(0.6);
  expect(knobDelta(0.95, -160)).toBe(1);
  expect(knobDelta(0.05, 160)).toBe(0);
});
test('faderValue', () => {
  expect(faderValue(150, 100, 200, false)).toBeCloseTo(0.25);
  expect(faderValue(150, 100, 200, true)).toBeCloseTo(0.75);
  expect(faderValue(50, 100, 200, false)).toBe(0);
  expect(faderValue(999, 100, 200, false)).toBe(1);
  expect(faderValue(150, 100, 0, false)).toBe(0);
});
test('formatTime', () => {
  expect(formatTime(257.34)).toBe('04:17.3');
  expect(formatTime(0)).toBe('00:00.0');
  expect(formatTime(-12.5)).toBe('00:12.5');
});
test('formatBpm and formatPitch', () => {
  expect(formatBpm(140)).toBe('140.00');
  expect(formatPitch(1.02)).toBe('+2.00%');
  expect(formatPitch(0.995)).toBe('-0.50%');
  expect(formatPitch(1)).toBe('+0.00%');
});
```

- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement `drag.ts` and `format.ts`; run the tests.** Expected: 4 passed.
- [ ] **Step 4: Implement the components and compose them in `App.svelte`** in the spike's layout: top bar 44 px, deck headers 104 px, waveforms flexible, controls 372 px.
- [ ] **Step 5: Add a round-trip test.** In smoke mode the front end calls `control(<deck 0 play id>, 1)` and awaits it before calling `ready`. In `crates/openmixer/tests/`, add `tauri_control_round_trip`: run `--ui=tauri --mock-device --smoke` and assert exit code 0; `gui_tauri` returns a failure exit code under `smoke` when `engine.snapshot().decks[0].playing` is `false` after `ready`. Expected: PASS.
- [ ] **Step 6: Screenshot** the full window with `--demo-scene=meters` and check: meters move and clip lamp reads, labels are not clipped at 1600 × 780, numerals are in the mono face, no literal colours stand out from the theme.
- [ ] **Step 7: Per-PR checks, commit, PR for Tasks 7 to 10.** `git commit -m "feat(ui-tauri): deck headers, transport and mixer"`

---

### Task 11: Measure and decide

**Files:**
- Create: `web/src/lib/bench.ts`, `web/src/lib/bench.test.ts`, `tools/ui-bench.sh`
- Modify: `crates/om-ui-tauri/src/{lib.rs,commands.rs}`, `crates/openmixer/src/gui_tauri.rs`, `docs/adr/0009-web-front-end-tauri-trial.md`, `docs/adr/SUMMARY.md`

**Interfaces:**
- Consumes: `RunOptions.bench_seconds` (Task 6).
- Produces:
  - `summarise(samples: number[]): { n: number; mean: number; p50: number; p95: number; p99: number; max: number }` — all zero for an empty list.
  - Flag `--ui-bench=SECONDS` (only with `--ui=tauri`): after a 3 s warm-up the front end records frame intervals and per-frame work for `SECONDS`, calls the command `bench_done(stats)`, and the shell writes JSON to the path in `OPENMIXER_BENCH_OUT` and exits. The shell adds `cpuPercentOneCore` and `rssMb`, summed over the process and its descendants with the `sysinfo` crate.
  - `tools/ui-bench.sh <gpui|tauri> <seconds>` — starts the chosen window with `--mock-device --demo-scene=track`, samples CPU of the process tree for the period, prints one line of JSON.

- [ ] **Step 1: Write the failing test:**

```ts
test('summarise', () => {
  const s = summarise([1, 2, 3, 4, 100]);
  expect(s).toMatchObject({ n: 5, mean: 22, p50: 3, max: 100 });
  expect(summarise([])).toEqual({ n: 0, mean: 0, p50: 0, p95: 0, p99: 0, max: 0 });
});
```

- [ ] **Step 2: Run to verify failure, implement `bench.ts`, run again.** Expected: PASS.
- [ ] **Step 3: Implement the flag, `bench_done` and `tools/ui-bench.sh`.**
- [ ] **Step 4: Run the measurements** on one machine with a working GPU path, 20 s each, with `OPENMIXER_DEMO_TRACK` set to a real track of at least four minutes:
  - `tools/ui-bench.sh gpui 20` — the baseline.
  - `tools/ui-bench.sh tauri 20` with frame caps 60 and off, playing and paused.
  - Tauri again at a maximised window.
  - Time from track load to waveform visible, in both windows.
- [ ] **Step 5: Apply the exit criteria.** The trial passes when all hold:
  - No frame interval above twice the display's refresh period in a 20 s playing run.
  - Mean per-frame work at or below 0.5 ms.
  - Paused CPU at or below 15% of one core.
  - Playing CPU at a 60 fps cap no more than 15 points above the GPUI baseline on the same machine.
  - Waveform visible within 500 ms of the analysis result being available.
  - Scrubbing, zooming and every control in the slice work by hand on Windows.
- [ ] **Step 6: Write the numbers into ADR 0009 under Evidence**, set its status to `Accepted` or `Rejected`, and update `docs/adr/SUMMARY.md`. If accepted, list plans 2 to 4 under Follow-ups. If rejected, open a PR that removes `crates/om-ui-tauri` and the `gui-tauri` feature and keeps `om-ui-bridge` only if something else uses it.
- [ ] **Step 7: Per-PR checks, commit, PR.** `git commit -m "docs: ADR 0009 trial results"`
