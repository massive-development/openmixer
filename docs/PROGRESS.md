# OpenMixer Progress & Parity Tracker

Where the project stands against the goal: a clean-room, djay-Pro-class DJ application, being rewritten in Rust with GPUI.
`TODO.md` is the task list; this page explains **what each task entails, what exists today, and what is still missing**. The "Entails" column is the behavioural spec each Rust task must meet.

C++ reference: see `cpp-final`.

Legend: ✅ done · 🟡 partial / in progress · ⬜ not started

_Last updated: Phase 0 complete; Phase 1 spikes run — Linux-first; GPUI gate closed on Linux, stretcher decided (Rubber Band R3 behind a Stretcher trait); Windows/macOS deferred to a porting phase. Update this file in the same PR that changes a task's status._

## At a glance

![OpenMixer progress heatmap](progress-heatmap.svg)

_Regenerate with `python3 tools/progress_heatmap.py` after editing `docs/progress.json`. Weighting: done = 1, partial = 0.5, in progress = 0.25._

| Wave | Area | Status |
|------|------|--------|
| 1 | Foundation | ✅ Rust scaffold (repo, Cargo workspace, CI, test harness) |
| 2 | Audio engine | ⬜ |
| 3 | Analysis | ⬜ |
| 4 | Library | ⬜ |
| 5 | UI | ⬜ |
| 6 | Hardware | ⬜ |
| 7 | Intelligence | ⬜ |
| 8 | Pro surface | ⬜ |
| 9 | Release | ⬜ |

**Rust rewrite status:** Phase 0 complete; Phase 1 spikes run — Linux-first; GPUI gate closed on Linux, stretcher decided (Rubber Band R3 behind a Stretcher trait); Windows/macOS deferred to a porting phase; see `docs/spikes/DECISIONS.md`; see the plan in `docs/superpowers/plans/2026-10-18-rust-phase0-1-scaffold-and-spikes.md`.

**What you can do today:** build the Rust workspace (`cargo build`), run the tests (`cargo test`), and check startup with `cargo run -p openmixer -- --smoke`. No audio, analysis, library or UI features exist yet.

---

## Wave 1: Foundation ✅

| Task | Entails | Status |
|------|---------|--------|
| Repo, governance, legal | AGPLv3 licence, clean-room ADRs (ADR 0004 records the Rust + GPUI stack), contributing guide and code of conduct, asset provenance record, licence check | ✅ |
| Build system | Cargo workspace (resolver 2, shared lints that forbid `unsafe` outside the FFI crate), `rust-toolchain.toml`, `cargo deny` licence policy | ✅ |
| CI | `.github/workflows/rust.yml`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` and `cargo deny` on Linux, macOS and Windows | ✅ green on Linux, macOS and Windows, plus `cargo deny` |
| Test harness | `cargo test`; `openmixer --smoke` mode (prints `smoke ok`, exit 0) checked by an integration test | ✅ |

**Missing:** packaging.

---

## Wave 2 — Audio engine ⬜

Goal: reliable, low-latency playback of two decks through a mixer, controllable by MIDI, testable headlessly.

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 7 Audio device layer | Device abstraction over `cpal` with latency/xrun instrumentation and a headless mock device for CI | — | All |
| 8 Deck engine | Streaming decode (`symphonia`), lock-free buffer, play/pause/seek/rate | — | All |
| 10 Mixer | Channel gain, 3-band EQ, filter, crossfader (curves), PFL/cue bus, master + headphone out | — | All |
| 10a Plugin API | Versioned plugin interfaces, runtime loader, registry; hooks for audio pre/post-processing, FX, MIDI maps. Deferred; CLAP-first if needed | — | All |
| 32 MIDI input + mapping | `midir` input → JSON map → engine parameters | Event, map JSON with typed feedback rules, mapper with hot reload, learn, trace replay, LED feedback, MIDI input and output routing over a device backend (ALSA sequencer on Linux, mock for tests), wired into the composition (`om-midi`, `openmixer::midi`) | midir backend for macOS and Windows (plugs in behind `system_backend`); the window's MIDI settings page does not call the learn and map controls yet (Todo 36 and the GPUI page) |
| 33 Controller trace harness | Replay recorded MIDI streams headlessly | Trace read/write and replay through the mapper (`om-midi`) | — |
| 26a Debug panel | Two decks, faders, EQ, meters on plain widgets | — | All; removed once Todo 28 lands |
| 13a Short soak test | 30–60 s headless mix in CI, asserting zero xruns, no steady-state underruns, no NaN/Inf/overflow | — | All; soak mix tool to be carried over from `cpp-final` |
| 9 Key-lock time-stretch | Tempo change without pitch change. Decided: Rubber Band R3 via FFI in `om-stretch`, behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `docs/spikes/DECISIONS.md`) | — | All |
| 12 Sync & quantize | Beat-phase alignment, quantized cue/loop, slip mode, gapless deck loops, 8 hotcues. Control ids `deck.N.sync`, `sync.master`, `deck.N.loop.quantized.<beats>`, `deck.N.slip` | — | All; UI buttons; hotcue persistence (Wave 4 library) |
| 11 FX rack | 8 effects (echo, reverb, flanger, phaser, crush, gate, roll, tape stop) with wet/dry and beat-sync; one rack per mixer channel and one on master (`fx.ch<N>.*`, `fx.master.*`); no allocation on the audio thread, buffer clears amortised | — | All; UI; beat-phase lock to the grid |
| 13 Full soak test | 10-minute headless mix with a scripted DJ (loops, hotcues, key-lock toggles, master FX, seeks); glitch and xrun budget (zero xruns, no audio-thread allocation) | — | All; not in CI (10 minutes); paced mock device, not real hardware |

**Parity gap vs. pro DJ apps in this wave:** sync wiring, FX UI, 4-deck support, recording-quality resampling.

---

## Wave 3 — Analysis ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 14 Analysis service + cache | Background job runner with progress, cancel and wait; versioned analyzer registry; SQLite result cache keyed by file content hash | All |
| 15 BPM + beat grid | Tempo estimation, beat grid with downbeat, manual correction (tempo markers); accuracy target: click tracks within 0.05 BPM and 3 ms | All; variable-tempo detection; other time signatures; wiring grids into decks and UI |
| 16 Key detection | Chroma-based key with Camelot mapping and harmonic compatibility; synthetic cadences in 8 keys detected | All; tuning-offset estimation (assumes A440); key changes within a track |
| 17 Waveform model | Multi-resolution overview and zoom data; low/mid/high band data for colouring; coarser levels rebuilt on load | All; stereo/per-stem waveforms; rendering (Wave 5); progressive results while analysing |
| 18 Structure features | Onset picking, energy curve, section boundaries with intro/main/breakdown/outro labels, mix-in/out points (feeds Automix) | All; bar-aligned boundaries; timbre/repetition features; real-music evaluation |
| 19 Stems precompute | Demucs-class separation via ONNX Runtime, weights downloaded on first use with the licence shown, SHA-256 verified | All; ONNX Runtime backend (`ort`); model export and hosted manifest; chunked overlap-add inference; stem storage keyed by content hash and separator version; UI licence dialog |

---

## Wave 4 — Library ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 20 Schema + DAO | Tracks, playlists, crates, cues (hotcues and saved loops in frames), play history; forward-only migrations; WAL and foreign keys | All; crates/tags; play history table; FTS index (Todo 23); app wiring (DB path in user data dir) |
| 21 Metadata ingest | Tag reading (ID3/Vorbis/MP4 and RIFF INFO), duration, artwork, recursive scan with content-hash change detection, missing-file flags, watch folders with background rescan | All; tag reader for MP3/FLAC/M4A (`lofty`); artwork; move detection by content hash; file-system change notifications (polling only) |
| 22 Import | iTunes and Rekordbox XML: ratings, play counts, playlists and folders, cues and loops from Rekordbox; single transaction with rollback; merge by path | All; Traktor and Serato importers; smart-playlist rules; artwork; path remapping when the library moved; Camelot from Rekordbox tonality |
| 23 Search + filter | Text and field queries (`artist:foo bpm:120-128 key:~8A added:7d -genre:techno sort:-bpm limit:50`) compiled to bound parameters; registry of searchable fields that plugins can extend | All; FTS (LIKE contains-match used if FTS5 is unavailable); saved smart filters and smart crates; ranking |
| 24 Prep queue | Analyse-on-import: persisted order and priority, prioritise/move/requeue/cancel, crash recovery, bounded in-flight analysis, results (BPM, key) written back to the library | All; auto-enqueue after ingest; waveform/structure results stored in the library; progress events; app wiring (timer pumping the queue) |

---

## Wave 5 — UI ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 25 Design tokens + LookAndFeel | Original visual identity (no copied layouts or artwork); runtime tokens for colour, spacing and type; a widget set covering knobs, faders, buttons, menus, text fields, toggles, tabs, scroll bars, table headers and tooltips, in dark and light themes; dB meters; translucent window option | All |
| 25a Theme/skin loader | JSON themes validated on load (missing keys inherit the dark theme; bad values reject the file); built-in, user-directory and plugin sources with precedence user > plugin > built-in; live switching; `OPENMIXER_THEME` override | All; layouts; file-watching of the user theme directory |
| 26 Main window + decks | Layout from data (two decks flank a mixer slot, library below; a 4-deck row optional); deck header (title, artist, BPM, key, elapsed/remaining); overview and zoomed waveforms with seek, grid lines, loop and cue markers; transport (play, cue, sync, master, key-lock, quantize, slip); pitch fader with ±8/16/50% ranges; 8 hotcue pads; loop size, halve/double, exit and reloop | All; a real main-cue point (Cue is currently "stop and return to start" in the baseline); visual check in a real session |
| 27 Mixer widgets + metering | Channel strips (trim, filter, 3-band EQ with kill, PFL, FX shortcut, L/Mid/R assign, fader, stereo peak meter with hold and clip); crossfader with Linear/Power curves; master and cue sections; FX panel per rack with unit select, enable, mix and knobs | All; RMS meters; FX tabs hiding the strips; sync-leader BPM entry |
| 28 Library browser + wiring | Sidebar tree (all tracks, playlists, folders, watch folders, prep queue); search box with the query syntax and debounce; virtualised, paged, sortable table with a harmonic-compatibility tint against the playing deck; Load buttons, double-click, drag onto a deck; import via file chooser; track loader that fills waveform and grid from cache or from the prep queue; `OPENMIXER_LIBRARY_DB` override | All; playlist editing; prep queue reorder and remove; auto-enqueue after a scan |
| 29 Automix panel + settings | Settings overlay with tabs for audio (driver, device, rate, buffer, master and headphone outputs, live latency/xrun/CPU), MIDI (per-input toggles, map file), library (watch folders, rescan, scan interval), appearance (theme picker), plugins (folder, loaded list, reload); versioned `preferences.json` applied at start-up and on change; `--settings` opens it at start | All; Automix panel (Wave 7, needs Todo 38); live scan-interval change; MIDI hot-plug refresh while open |
| 30 Stem pads UI | Per-stem mute and volume | All; waits on Wave 7 stem playback |
| 31 UI verification gate | Scripted DJ operates the full window only through widgets (search and load, transport, crossfader, EQ, hotcues, loops, FX, theme switch) on a paced mock device; after each action the widget and engine must agree; xrun, underrun, NaN and overflow budgets; 10-minute opt-in run; `--smoke` opens the window and exits 0 (skipped without a display) | All; real display and audio device; overview seek, settings and drag-and-drop not in the scenario; stem pads not covered |

---

## Wave 6 — Hardware ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 34 Controller maps | Plug-and-play maps for popular controllers | All |
| 35 HID layer | Non-MIDI controllers | All |
| 36 Mapping editor UI | In-app learn and edit | All |

## Waves 7–9 ⬜

| Wave | Contents | Status |
|------|----------|--------|
| 7 | Stem-aware mixing, Automix, stem transitions, 8-pad sampler | ⬜ |
| 8 | Mix recording, broadcasting, DVS (timecode vinyl), video mixing | ⬜ |
| 9 | Legal audit, trademark check, packaging, docs, v1.0.0 | ⬜ |

## Deferred C++ test ports

Tests from the C++ suite that are not ported yet, each with the Todo that owns it. A line is removed when its port lands.

- `MeterBallisticsTest.DefaultsComeFromTheTheme` (`tests/unit/MeterBallisticsTest.cpp`): deferred to Todo 25 (design tokens + LookAndFeel). The meter defaults are read from the theme, which belongs to the widget layer. <!-- cpp-history -->
- `MeterBallisticsTest.MeterBarFollowsUpdatesPaintsAndClearsClipOnClick` (`tests/unit/MeterBallisticsTest.cpp`): deferred to Todo 27 (mixer widgets + metering). It paints the bar and handles the clip click, so it needs the widget. <!-- cpp-history -->
- `MeterBallisticsTest.StereoMeterFeedsEachSideSeparately` (`tests/unit/MeterBallisticsTest.cpp`): deferred to Todo 27 (mixer widgets + metering). It exercises the stereo meter widget. <!-- cpp-history -->

---

## Known out-of-scope / non-goals

- Spotify or other streaming-service integration (terms of service prohibit DJ mixing).
- Mobile and web apps.
- Real-time stem separation (stems are precomputed offline).
- Webview-based UI (no webview, Electron or Tauri).

## How to help

1. Pick a ⬜ or 🟡 row, open an issue naming the Todo number.
2. Read `CONTRIBUTING.md` and `docs/adr/0001-clean-room-doctrine.md` before writing code.
3. Include tests; engine code must not allocate or lock in audio callbacks.
