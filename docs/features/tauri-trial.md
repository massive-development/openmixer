# Web front end: the Tauri window (ADR 0009)

## What it is

A second window for the decks screen (waveforms, deck headers, transport, hot cues, loops, mixer and meters), built with Tauri v2 and a Svelte 5 front end, running on the real engine beside the GPUI window. It was built as a trial by the plan [`docs/superpowers/plans/2026-11-15-tauri-frontend-slice.md`](../superpowers/plans/2026-11-15-tauri-frontend-slice.md), and [ADR 0009](../adr/0009-web-front-end-tauri-trial.md) was accepted on 2026-11-16 by the project owner's decision: the web front end is the direction for the interface. GPUI stays the default and the only shipping window until the cut-over plan lands.

- Behind the `gui-tauri` cargo feature of `openmixer` (off by default) and the `--ui=tauri` flag.
- Opens the stored output device (else the default output), as GPUI does; a device that does not open falls back to the silent mock with GPUI's no-device notice. `--mock-device`, `--smoke`, `--ui-bench`, `--ui-drive-out` and the demo flags always run on the mock, so tests and tooling never reach a sound card. No MIDI, plugin state or settings page reach it yet, so the device cannot be changed from this window.
- The installers (`tools/package.sh`, [Installers](#installers)) build this window as the app, with no GPUI.
- The trial's measurement (plan Task 11, steps 4 and 5) was not run before acceptance; its exit criteria are kept in the ADR as targets for the migration. The only numbers so far are tooling checks under WSL software rendering.

## Build and run

Linux packages: `libwebkit2gtk-4.1-dev` on top of the usual ones (see [build.md](../guidelines/build.md#tauri-window-web-front-end)). Node: 22 (22.12 or newer), 24, or 26 and later (Vite 8 and Vitest 5 refuse older; Vitest also refuses 23 and 25). CI uses Node 22.

```bash
cd crates/om-ui-tauri/web && npm ci && npm run build && cd -    # writes web/dist
cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device
cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device --demo-library
cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device --demo-scene=cues
cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device --smoke
```

- `npm run build` first runs `npm run fonts`, which copies `resources/fonts/` into `web/public/fonts/` (ignored by git), then `vite build` into `web/dist/` (ignored by git).
- The front end is embedded at compile time (`tauri::generate_context!`), so rebuild `web/dist` before the cargo build that should show a front-end change. Without `web/dist`, `build.rs` writes a placeholder `index.html` so the crate compiles; the window is then blank and `--smoke` fails.
- Demo flags it honours: `--demo-library` (two generated tracks into decks A and B, from a library in the scratch folder `openmixer-smoke-<pid>` under the temp dir) and `--demo-scene=NAME` (the same scenes as GPUI, staged on deck A; implies the demo library). Without either the decks are empty and no library is opened. `--demo-map`, `--demo-automix` and `--demo-record` are not read by this window. The scratch folder is removed when the window closes, unless it holds a bench report.
- Theme: the saved theme, or `OPENMIXER_THEME=<theme id>` for the session (`openmixer.dark`, `openmixer.daylight`, `openmixer.harbour`). The density is resolved once for the opening size, 1600 × 780 (`tauri.conf.json`), with `main_layout::resolve_density`. The theme is sent once at start; there is no live theme switch.
- On Linux the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` unless it is already set (`om_ui_tauri::prepare_environment`, called first in `gui_tauri::main`). Under WSL run it with `WAYLAND_DISPLAY=` as other windows.

## The two crates

| Crate | Owns | Depends on |
|---|---|---|
| `om-ui-bridge` | Toolkit-free conversions, tested headless, no `unsafe`: the snapshot packet (`pack_snapshot`, `packet_len`), packed waveform peaks (`pack_peaks`, `peak_levels`), the slice's control keys and ids (`slice_control_ids`) and their specs from the GPUI widget models (`slice_controls`), a knob's printed value (`caption_text`), validated dispatch (`dispatch_control`), and theme tokens as CSS (`theme_css`). Errors: `BridgeError`. | `om-core`, `om-analysis`, `om-ui` |
| `om-ui-tauri` | The Tauri v2 window: the commands, the `deck-changed` event, the stream thread, the control thread that pumps the demo library and track loader, strip ink, the bench report, the `--ui-drive-out` layout map and state file, and the smoke watchdog. The Svelte front end is in `web/`, the icons in `icons/`. No `unsafe`. Errors: `ShellError`, `BenchError`, `DriveError`. | `om-ui-bridge`, `om-engine`, `om-ui`, `om-analysis`, `om-core`, `tauri` 2 |

The composition (engine, router, theme, demo library) comes from the binary: `crates/openmixer/src/gui_tauri.rs`, using the start-up steps the GPUI window also uses (`openmixer::startup`: mock output, starting the output, demo tracks and decks). `main.rs` dispatches `--ui=tauri` to it.

## Data flow

### Snapshot stream (out)

`start_stream` starts the thread `om-ui-stream`, which every 1/30 s pulls `DjEngine::snapshot()`, packs it with `pack_snapshot` into one reused buffer and sends the bytes through a Tauri `Channel`. A deadline pacer advances by one period per packet and resets when it falls behind, so a late tick never becomes a burst. The front end decodes each packet (`web/src/lib/packet.ts`), drops bytes that do not decode, and interpolates the playhead between packets (`clock.ts`: position + elapsed × file rate × rate, wrapped inside an active loop, clamped to the track).

Packet layout (version 2, little-endian; `packet_len(2, 2)` is 216 bytes). Offsets are from the start of each block.

| Block | Offset | Size | Type | Field |
|---|---|---|---|---|
| Header (24) | 0 | 4 | u32 | magic `0x4F4D5331` |
| | 4 | 2 | u16 | version, `2` |
| | 6 | 1 | u8 | deck count (saturates at 255) |
| | 7 | 1 | u8 | channel count (saturates at 255) |
| | 8 | 8 | u64 | `EngineSnapshot::ticks` |
| | 16 | 4 | f32 | `age_ms`: milliseconds `DjEngine::snapshot()` took on the stream thread |
| | 20 | 4 | u32 | reserved, 0 |
| Deck (56 each, at 24 + 56 × i) | 0 | 8 | i64 | position, source frames |
| | 8 | 8 | i64 | length, source frames |
| | 16 | 8 | f64 | rate |
| | 24 | 8 | f64 | file sample rate |
| | 32 | 1 | u8 | flags: bit 0 loaded, 1 playing, 2 looping, 3 sync, 4 keylock |
| | 33 | 7 | | padding, 0 |
| | 40 | 8 | i64 | loop start, source frames |
| | 48 | 8 | i64 | loop end, source frames |
| Channel (32 each, after the decks) | 0 | 4 × 8 | f32 | peak left, peak right, fader, trim, filter, EQ low, EQ mid, EQ high |
| Master (16, last) | 0 | 4 × 4 | f32 | peak left, peak right, master gain, crossfader |

A wrong magic, a wrong version or a buffer shorter than the counts say is refused. The golden packet `fixtures/ui-bridge/snapshot-v2.bin` is asserted by the Rust tests and decoded by the TypeScript tests; `cargo test -p om-ui-bridge write_golden -- --ignored` rewrites it. Bump `PACKET_VERSION` (both sides) whenever the layout changes.

### Controls (in)

1. At start the front end calls `controls` once. It returns a map from front-end key to spec: `id` (canonical `ParamId` string), `min`, `max`, `default`, `detent` (or null), `kind` (`toggle`, `trigger` or `continuous`), `label` (the word GPUI prints, or empty), `law`, `ticks`, `snap`, `readout` and `writes` (`[{ key, value }]`, empty except for a composite key). `om_ui_bridge::slice_controls` reads every range, default, detent and label off the `om-ui` widget models the GPUI window builds (`ChannelStrip`, `MasterSection`, `CrossfaderSection`, `TransportButton`, the loop sizes); tempo is the pitch fader's narrowest range (0.92 to 1.08, default 1.0) and seek is 0 to 1 of the track. The other four come from the definitions GPUI applies (`om_ui::fader_law`, `om_ui::control::snap_with_bypass`, `om_ui::control::Readout`):

   | Field | Shape | Carried by |
   |---|---|---|
   | `law` | `{ "kind": "linear" }`, or `{ "kind": "knots", "knots": [[position, value], ...] }` with positions rising from 0 to 1 | Knots: the channel faders (`FaderLaw::Channel`) and the master (`FaderLaw::Master`), sampled at every 1% of travel; GPUI's laws are linear in dB between their own knots, so interpolating the samples stays within 0.01 dB of them. Linear: every other control, where a detent strictly inside the range still takes the middle of the travel. |
   | `ticks` | `[{ "pos", "label", "major", "unity" }]`, loudest first | The channel faders and the master: every dB mark of `om_ui::fader_law::marks`, the label only where GPUI prints one (the major marks). Empty for every other control. GPUI also drops marks closer than 12 px on a short fader; the front end does not. |
   | `snap` | `{ "at", "width" }` or null | Trim, filter and EQ: the knob's detent and 3% of its range (`Knob::detent_width`). Null elsewhere, the master included (GPUI drags it as a fader, which never pulls). |
   | `readout` | `"gain"`, `"decibels"`, `"filter"` or null | The knobs: the name of the `om_ui::control::Readout` of the GPUI model (`Knob::readout`; trim and master `gain`, EQ `decibels`, filter `filter`), from `om_ui_bridge::readout_name`. Null for every control that is no knob. |

   The front end maps travel through `law` both ways (`lib/controls/range.ts`), so a drag, an arrow key (1% of travel, 10% with Shift or a page key) and the thumb's place for a snapshot value all follow the same taper; the master knob turns through the master taper too, and its detent mark sits at unity's angle on it. `fixtures/ui-bridge/fader-law.txt` holds the knots and 18 travel positions with their gains; `om-ui-bridge`'s `tests/fader_law_table.rs` checks it against the specs and `FaderLaw`, and `range.test.ts` against its interpolation (`cargo test -p om-ui-bridge --test fader_law_table write_table -- --ignored` rewrites it).
   A knob prints its value under its label, as GPUI's mixer does: `om_ui_bridge::caption_text`, the readout's `Readout::text` without the unit the label names (GPUI's `caption_value` in `mixer_view.rs`): `0`, `+6`, `-inf`, `KILL`, `Off`, `LPF 50%`. The front end ports the three slice readouts (`lib/controls/readout.ts`, `captionText`; the knob's `aria-valuetext` is `spokenText`, the same reading with its unit and sign in words); `fixtures/ui-bridge/knob-text.txt` holds 37 (key, readout, value, text) rows that `tests/knob_text_table.rs` regenerates from `caption_text` and `readout.test.ts` checks against the port (`cargo test -p om-ui-bridge --test knob_text_table write_table -- --ignored` rewrites it). The value sits under the label in the mono face at the label size, `--om-text-secondary`; the master's sits under the master heading. While dragged it shows the local value. The label alone sets the knob column's width: the filter's `LPF 5%` to `HPF 100%` (33 to 44 px at 11 px) are wider than `FILTER` (32 px) and run out into the gap towards the fader, on the side away from the knob.
2. Keys: per deck `deck<d>.play`, `.sync`, `.tempo`, `.seek`, `.hotcue<0..7>`, `.hotcue<k>.clear`, `.loop.<beats>` (0.25, 0.5, 1, 2, 4, 8, 16, 32) and `.loop.exit`; per channel `ch<c>.fader`, `.trim`, `.filter`, `.eq.low`, `.eq.mid`, `.eq.high`; and `mixer.crossfader`, `mixer.master`. For example `deck1.tempo` is `deck.1.rate` and `deck1.loop.0.25` is `deck.1.loop.quantized.0.25`.
   - Composite keys: `deck<d>.cue`, one per deck, after the single keys. The engine has no control id for Cue, so the spec's `id` is empty and its `writes` are GPUI's Cue (`TransportPanel::press(TransportButton::Cue)`: with no main-cue point in the engine yet it stops the deck and returns it to the start), in order: `deck<d>.play` 0, then `deck<d>.seek` 0. The label is `TransportButton::Cue`'s. The front end sends each write with `control`, awaiting one before the next (`lib/deck/transport.ts`); a failed write stops the rest. `om-ui-bridge` checks the writes against what GPUI's Cue sends through a recording target.
3. A control writes with `control(key, value)` in `ipc.ts`: an unknown key is refused in the front end without calling the shell; otherwise the `control` command receives the spec's id and the value. Toggles send the opposite of the state the snapshot reports, triggers send 1, continuous controls send engine units.
4. `dispatch_control` rejects a non-finite value, parses the id with `ParamId::from_str`, validates it, and dispatches it once through the `ParameterRouter`; any failure returns an error string and dispatches nothing. It runs in the `control` command, which Tauri runs on the main thread, never on the audio thread. The command accepts any valid id, not only the slice's.

### Deck data

- `deck_info(deck)` returns the track (`title`, `artist`, `bpm`, `key`), the header words GPUI's `TrackHeader` prints (`titleText`, `keyText`), `firstBeatFrame` (the beat grid's beat 0), `sampleRate`, `numSamples`, `spectral`, the peak level table `levels` (`samplesPerPoint`, `len` per level), `hotcues` (eight frames or null), `generation` (counts every change: track, waveform, hot cues) and `peaksGeneration` (counts waveform changes only: a new `WaveformData`, or going to or from none). A beat grid's BPM takes precedence over the track's, as in GPUI.
- `deck_peaks(deck)` returns raw bytes (an `ArrayBuffer`): first the deck's `peaksGeneration` as a little-endian u64 (8 bytes, read together with the peaks so a late answer is recognised), then every level of the track's `WaveformData`, finest first, back to back, four bytes per point: body, low, mid, high. Each byte is the height (0 to 1 of the half-height, before `FILL`) the GPUI Low-Band strip draws for that point, normalised by the track's reference, times 255. With `spectral` false the whole height is in the low byte and the lane is a colourless silhouette. No peak bytes (only the 8-byte generation) while the deck has no waveform.
- `deck-changed` (payload `{ deck, generation }`) is emitted from the control thread when a deck's track, waveform or hot cues change; the generation counts the changes. Installing the demo library's track loader measures every deck under the same lock and emits the changes at once, so `deck_peaks` read before the first tick already carries the generation of the waveform it returns. The deck headers, panels, lanes and overview reload `deck_info` on every event, so cue flags, cue ticks and titles follow at once. The lanes and the overview call `deck_peaks` (and the lanes upload a texture, the overview redraws its canvas) only when `peaksGeneration` differs from the one they last showed; a hot cue or title change fetches no peaks. An answer whose generation is not the newest the deck info named is dropped, so a late answer never replaces newer peaks (`lib/peaks.ts`, `PeakSync`).

### Strip ink and waveform style

`strip_ink(deck)` returns one deck's Low-Band lane colours and marker sizes, built once per deck before the window opens by `om_ui_tauri::strip_ink_dto` from the same `om_ui::waveform::StripInk` the GPUI strip uses: `well`, `body`, `highs` (`#rrggbb`) and `highsAlpha`, `playedShade` and `playedShadeAlpha` (overview only; null when the style dims the played part instead), grid bar and beat colours with alphas, `playhead`, `playheadHalo`, their widths, `cueGuide` and its alpha, `mainCue`, the eight hot cue colours, and the bar number and cue flag sizes. `waveform_style` returns the GPUI strip's `fill`, `playedBrightness` and `minHalfPx`. The shader and the overlays carry no colour of their own.

### Theme CSS

`theme_css` returns one `:root { ... }` block that the front end puts in a `<style id="om-theme">` before its first paint. Naming and units are in [styling.md](../styling.md#the-web-front-end-tauri-window).

## Threads

| Thread | Name | Runs | Stops |
|---|---|---|---|
| Main | (Tauri event loop) | the window and the synchronous commands (`theme_css`, `run_config`, `bench_done`, `smoke_play`, `ready`, `waveform_style`, `strip_ink`, `control`, `controls`, `ui_layout`) | when the window closes or the app exits |
| Tauri async runtime | (Tauri's) | `start_stream`, `deck_peaks`, `deck_info` | with the app |
| Stream | `om-ui-stream` | packs and sends snapshot packets at 30 Hz | when `start_stream` replaces it, when the sink fails (the webview is gone), or when `run` closes the stream slot after the window closes; it is joined |
| Control | `om-ui-control` | builds the demo library (the library services cannot move between threads), then at 30 Hz pumps the services and the track loader and emits `deck-changed`; with `--ui-drive-out`, every third tick (10 Hz) it rewrites the state file from `DjEngine::snapshot()` | when `run` sets its stop flag after the window closes; it is joined |
| Watchdog | `om-ui-smoke` | smoke and bench runs only: waits for `ready` (60 s) or the bench report (60 s plus the bench length) | when the signal arrives; on timeout it exits the app with code 1 |

Nothing in the web window runs on the audio thread or waits on a plugin.

## Commands and events

| Name | Signature (front end) | Returns |
|---|---|---|
| `theme_css` | `themeCss()` | the `:root` block of `--om-*` variables |
| `run_config` | `runConfig()` | `{ smoke, benchSeconds, fpsCap, banner, hud, drive }`: `banner` the mock-device notice or null, `hud` whether the frame counter shows, `drive` a `--ui-drive-out` run |
| `ready` | `ready()` | nothing; marks the front end ready, and a smoke run exits 0 |
| `smoke_play` | `isSmokePlay()` | whether a smoke run must start deck A through `control` first (true with a library) |
| `bench_done` | `benchDone(stats)` | nothing; checks and writes the bench report, then exits 0, or 1 when refused |
| `waveform_style` | `waveformStyle()` | `{ fill, playedBrightness, minHalfPx }` |
| `strip_ink` | `stripInk(deck)` | the deck's `StripInk`; an error for an unknown deck |
| `controls` | `loadControls()` | key to `ControlSpec`; loaded once, retried after a failure |
| `control` | `control(key, value)` (sends `{ id, value }`) | nothing, or the dispatch error |
| `start_stream` | `startStream(onSnapshot)` (sends a `Channel`) | nothing; packets arrive on the channel |
| `deck_info` | `deckInfo(deck)` | `DeckInfo`; an error for an unknown deck |
| `deck_peaks` | `deckPeaks(deck)` | `{ generation, bytes }`: the peaks generation and the packed peaks |
| `ui_layout` | `uiLayout(map)` | nothing; `--ui-drive-out` runs only (else an error): checks the layout map (see Driving the window) and writes it with every control's spec |
| event `deck-changed` | `onDeckChanged(cb)` | `{ deck, generation }` |

The window's capability (`capabilities/default.json`) grants `core:default` to the `main` window; the app's own commands are allowed by `tauri-build`. The content security policy in `tauri.conf.json` allows only the app's own scripts, styles, fonts and images, inline styles (`style-src 'unsafe-inline'`) and the IPC endpoint.

## Rendering, as implemented

- **One shader for the zoomed lanes.** One WebGL2 canvas and one program (`waveform/shader.ts`, `gl.ts`) draw every deck's lane: one full-lane quad per deck, placed by the viewport. The shader picks a peak level for the zoom (`view.ts`, `pickLevel`) and takes the maximum over up to eight points per pixel. It draws the well, the body in the deck colour, the high band over it (body × high, so never taller than the body), the beat grid (bar lines stronger than beat lines; beat lines only when at least 10 px apart) and the playhead fixed at the centre. The canvas is drawn only when a lane moves, so its context keeps the drawing buffer (`preserveDrawingBuffer`, `CONTEXT_OPTIONS`): without it WebKitGTK showed a stopped deck's lane blank once it re-composited the page.
- **Peaks as a texture.** Each deck's levels are uploaded as one RGBA8 texture 4096 texels wide, each level starting on its own row. A lane uploads once per waveform: only when the deck's `peaksGeneration` moves, never for a hot cue or title change, and it stops drawing the old track's peaks as soon as the info names a new waveform.
- **The overview** is a 2D canvas drawn once per track and size; its played shade and playhead move by CSS transform.
- **DOM overlays by transform.** Bar numbers (64 pooled elements per lane) and cue flags (8 per lane) move with `translate3d` and show or hide only when that changes; a bar number's text is written only when its number changes. Meters move one cover per bar with `scaleY`, fall at most 3% of the height per frame, and are written only when they moved more than 0.3%.
- **One frame loop.** `ticker.ts` runs the one `requestAnimationFrame` loop: the lanes run every frame; the meters, the overview and the deck read-outs (elapsed, remaining) are woken by a snapshot or an input and stop when idle. Knobs and faders follow the snapshot through Svelte's own updates.
- **Skip unchanged frames.** The lanes draw only when a lane's position or zoom changed, or after new data or a resize.
- **Frame cap.** The lanes draw at most `--ui-fps-cap` times a second (default 60, 0 uncapped), with 2 ms slack (`view.ts`, `shouldDraw`). The cap applies to the lanes only.
- **Text only on change.** Read-outs go through `dom.ts` (`setText`, `setStyle`), which write only a changed value. Values that only assistive technology reads are paced: meters at most every 250 ms, the overview position once a second.
- **Interaction.** Dragging a lane scrubs (sent as `deck<d>.seek`); the wheel zooms between 1 and 64 visible seconds (default 8). Clicking or dragging the overview seeks. Each lane also takes keys (see Accessibility): a key's seek goes through the same scrub hold and send as a drag's, and its zoom changes the same shared view.
- **Top bar.** The mock-device notice (`run_config().banner`, the text of `om_ui::labels::Notice::Demo`, which points at a Settings screen this window does not have yet; see the ADR's known gaps), centred, one line, `role="status"`, cut with an ellipsis; nothing when null. GPUI's rule (`gui.rs`): every run here is on the mock device, and a demo run (`flags::demo_run`: `--demo-library`, `--demo-scene`, `--demo-map`, `--demo-automix`, `--demo-record`) shows none. A frame counter (`NN fps · N.NN ms`, drawn frames and mean work) shows only when `run_config().hud` is true (`--ui-bench` or `--debug-panel`); otherwise the ticker makes no counter window, so nothing is counted, formatted or written. It is hidden from assistive technology.

## Front-end map

All under `crates/om-ui-tauri/web/src/`.

| Module | What it does |
|---|---|
| `main.ts`, `App.svelte`, `app.css` | Mounts the app; start order (run config, theme, controls, first paint, stream, `ready`, bench); the page grid; the `@font-face` rules and base styles |
| `lib/packet.ts` | Packet decode |
| `lib/clock.ts` | Playhead interpolation |
| `lib/ipc.ts` | Every command and event, typed |
| `lib/ticker.ts`, `lib/hud.ts`, `lib/paint.ts`, `lib/dom.ts` | The frame loop, the frame counter (`HudWindow`, made only when the run shows it), waiting for the first paint, write-on-change helpers |
| `lib/bench.ts` | Frame recorder and the bench summary |
| `lib/drive.ts` | The `--ui-drive-out` layout map: every `data-control` element's rectangle |
| `lib/theme.ts` | Puts the theme CSS in its style element |
| `lib/decks.svelte.ts` | Per-deck track info and ink, reloaded on `deck-changed` |
| `lib/peaks.ts` | The `deck_peaks` answer and `PeakSync`: when peaks are fetched (a new `peaksGeneration` only) and which answer is shown |
| `lib/meter.ts` | Meter scale, zones, fall and write threshold (`om_ui::meter::MeterScale`) |
| `lib/a11y.ts` | Accessible names and value texts |
| `lib/format.ts` | Read-out formatting |
| `lib/controls/` | `Knob`, `Fader`, `Button`, `Meter`; `readout.ts` (a knob's printed value, `om_ui_bridge::caption_text`), `drag.ts` (drag travel and Shift's fine mode, hold while the engine catches up), `keys.ts` (slider keys, through the law), `range.ts` (value and travel through the spec's `law`), `snap.ts` (a dragged knob's detent pull) |
| `lib/deck/` | `DeckHeader`, `Overview`, `DeckPanel`, `DeckTag`, `HotcuePads`, `LoopRow`, `Transport`; `overview.ts`, `pads.ts`, `transport.ts` (Cue's writes, sent in order) |
| `lib/mixer/` | `Mixer`, `ChannelStrip` |
| `lib/waveform/` | `Waveforms.svelte` (lanes and overlays), `LaneSlider.svelte` and `sliders.svelte.ts` (each lane's keyboard slider and what it shares with the lanes), `gl.ts`, `shader.ts`, `view.ts` (zoom, level choice, grid, frame cap), `scrub.ts`, `lanekeys.ts` (`laneKey`: what a key does to a lane) |
| `lib/icons/` | `icons.ts`, `Icon.svelte`, `LICENSE-LUCIDE` |

Layout: a top bar, the two deck headers (with their overviews), the waveform lanes, then deck A's panel, the mixer and deck B's panel. Source order is deck A, mixer, deck B, which is the reading and Tab order; each lane's keyboard slider sits in the source right after its deck's header (so after its overview) and is laid over its lane in the waveforms row.

Deck B is deck A's mirror image, as in GPUI, by layout only (a `mirrored` prop that switches grid areas or the flex direction): both decks have one source order, so Tab and a screen reader meet header, overview, lane, tempo fader, read-outs, hot cues, loops and transport in the same sequence on each. There is no automated tab-order test (Vitest runs without a DOM); the check is that `DeckHeader`, `DeckPanel` and `Transport` branch on `mirrored` only in classes, never in markup.

- Header: deck A has its letter at the outer (left) edge, then title and artist, then Key, BPM and Remain reading towards the centre; deck B has Remain, BPM and Key from the centre, then title and artist right-aligned, and its letter at the outer (right) edge. The text inside each read-out reads left to right on both; deck B's are left-aligned where deck A's are right-aligned.
- Panel: the tempo fader on the outer edge, with no read-out of its own (it keeps its value text); the read-out row (deck tag, BPM, Pitch, Elapsed) runs from the outer edge on both decks, right-aligned on deck B. The pitch shows once, there, beside the BPM, as GPUI shows it once.
- Transport row: deck A Cue, Play/Pause (the wider track), Sync; deck B Sync, Play/Pause, Cue, so Play and Cue are on each deck's outer side.
- Hot cue pads (1 to 8) and loop sizes (1/4 to 32) read in the same order on both decks, as in GPUI.

## Accessibility, as implemented

| Control | Role and name | Value text | Keys |
|---|---|---|---|
| Knob (trim, EQ, filter, master) | `slider`, "Channel A high EQ", "Master gain" | The reading under the label, spoken with its unit and sign in words (`spokenText` in `controls/readout.ts`, from the spec's `readout`): `0 dB`, `plus 3 dB`, `minus 3.5 dB` (tenths of a decibel), `minus infinity`, `kill`, `off`, `low-pass 40 percent`; the visible caption stays GPUI's (`+3`, `-inf`, `KILL`, `LPF 40%`) | Arrows 1% of travel (Shift 10%), Page Up and Down 10%, Home and End the ends, Delete or Backspace the default; double-click the default. A drag of trim, EQ or filter sticks to the detent within its `snap` width once the value has moved, as in GPUI. Shift held at the press makes the whole drag fine, as GPUI's `begin_drag_fine`: a tenth of the travel per pixel (`FINE_DIVISOR` 10) and no detent pull; Shift pressed or released mid-drag changes nothing. The master knob drags fine too, although GPUI drags the master as a fader. Keys never stick, so one arrow step leaves the detent |
| Fader (channel, crossfader, tempo) | `slider` with `aria-orientation`, "Channel A fader", "Crossfader", "Deck A tempo" | `85 %` of linear gain; crossfader `centre` or `A 30 %`; tempo `+2.00 %` | As the knob, through the fader's law; the tempo fader ignores Delete, Backspace and double-click, as in GPUI. A channel fader draws its `ticks` (dB labels on the knob side, mirrored on deck B's strip); the tempo fader keeps evenly spaced marks |
| Overview | `slider`, "Deck A position"; disabled without a track | `1:02 of 2:04` | Arrows 1% of the track (Shift 10%), Page keys 10%, Home and End; no reset |
| Waveform lane | `slider`, "Deck A waveform"; after the deck's overview in the Tab order | `1:02 of 2:04`, at most once a second (at once after a key) | Left and Right one beat on a grid (`bpm` and `firstBeatFrame`), else 0.5 s; with Shift four beats, else 2 s; Home and End the track's ends; seeks clamp to the track as the scrub does. `+` or `=` zoom in and `-` out by one wheel notch (×1.15, 1 to 64 s), `0` back to 8 s (the zoom is shared by both lanes, as the wheel's). Other keys, and any key with Ctrl, Alt or Meta, pass on. The focused lane draws the theme ring inside its edges |
| Cue, Play, Sync | `button`, "Deck A cue", "Deck A play" or "Deck A pause", "Deck A sync"; Sync has `aria-pressed`; Cue never stays lit | | Space or Enter |
| Hot cue pads | `button` in the group "Hot cues", "Deck A hot cue 3, set at 0:01.0" or "..., empty" | | Space or Enter sets or jumps; Delete or Backspace clears; Shift, Alt or Ctrl click clears |
| Loop sizes | `button` with `aria-pressed` in the group "Loop", "Deck A loop 1/4 beat" | | Space or Enter; a lit size ends its loop |
| Meters | `meter`, "Channel A level", "Master level" | whole dB or `silent`, paced to 250 ms | |
| Regions | `section`s "Deck A track", "Deck A controls", "Mixer", "Waveforms"; channel strips and master are labelled groups | | Tab follows source order |

The zoomed lanes' canvas carries a text summary ("Scrolling waveforms of Deck A and Deck B, top to bottom"); bar numbers, cue flags, deck letters and the frame counter are hidden from assistive technology. Focus uses the theme's ring (`--om-focus-ring`, `--om-focus-border-width`). With `prefers-reduced-motion` the hover and press transitions are off; meters and waveforms keep moving. The mock-device notice is a `status`.

## Icons

- Control glyphs: five Lucide icons (ISC; `web/src/lib/icons/LICENSE-LUCIDE`), the same set the GPUI window draws from: `play`, `pause`, `refresh-cw` (Sync), `repeat` (Loop) and `flag` (a set hot cue). `icons.ts` holds each SVG's inner markup unchanged (24 px grid, 2 px stroke), taken from `gpui-kit-assets` 0.8.1 and recorded in `docs/legal/asset-provenance.csv`; `Icon.svelte` draws them in `currentColor`, hidden from assistive technology.
- App icon: `icons/openmixer-icon.svg` (48 px and up) and `icons/openmixer-icon-small.svg` (32 px and below), the OpenMixer mark from the App Icon design comp (tile 15a) in the colours of the project website's favicon. `icons/render.sh` renders the `.png`, `.ico` and `.icns` files (librsvg, ImageMagick for the `.ico`, Python with Pillow for the `.icns`); do not edit the rasters by hand. The first `.png` in `tauri.conf.json`'s `bundle.icon` is the Linux window icon. The `.icns` has not been checked on macOS.

## Installers

`tools/package.sh` builds the installers for the OS it runs on into `target/packages/<os>/`: Linux `.deb`, `.rpm` and `.AppImage`; Windows `.msi` and setup `.exe` (from Git Bash); macOS `.app` (zipped) and `.dmg`. It runs `npm ci` and `npm run build` in `web/`, builds `openmixer` with `--release --no-default-features --features tauri-default` (plus `realtime-dbus` on Linux), so the app opens this window with no flag and carries no GPUI, then runs the Tauri CLI's `tauri bundle` (pinned in `web/package.json`) with `tauri.bundle.json` merged over `tauri.conf.json`. `mainBinaryName` there points the bundler at the workspace's `openmixer` binary; the base file keeps `bundle.active` off, so plain cargo builds bundle nothing.

```bash
tools/package.sh                    # this OS, every bundle type
tools/package.sh --bundles deb,rpm  # some types
tools/package.sh --docker           # Linux packages in a Debian 12 container (tools/package/Dockerfile)
```

- `--docker` keeps its own target folder (`target/docker`), cargo registry, rustup and `node_modules` (Docker volumes named `openmixer-package-*`), so the host's stay untouched, and hands the files it wrote back to the calling user. `CARGO_BUILD_JOBS` passes through.
- macOS packages cannot be built on Linux or Windows. `.github/workflows/package.yml` runs the script on Ubuntu 22.04, Windows 2022 and the latest macOS (Apple silicon) on demand, on a `v*` tag and on a pull request that changes the packaging, and uploads the packages as workflow artifacts.
- The bundler rewrites `om-ui-tauri/Cargo.toml`'s `tauri` and `tauri-build` lines to `{ version = "2", features = [] }`; the file is kept in that form so a run leaves the tree clean.
- Every run writes `SHA256SUMS-<os>.txt` beside the packages (`tools/package/checksums.sh`).

### Signing

Each step runs only when its keys are present, so a fork, a pull request or a machine without them builds the same packages unsigned.

| What | How | Keys (`package.yml` secrets; locally, environment variables) |
|---|---|---|
| Checksums, every OS | `SHA256SUMS-<os>.txt.asc`, a detached GPG signature | `GPG_PRIVATE_KEY` (armoured), `GPG_PASSPHRASE`; locally `OPENMIXER_GPG_KEY` (a key in your keyring) and `OPENMIXER_GPG_PASSPHRASE` |
| AppImage | A GPG signature embedded by the AppImage tool (Tauri's `SIGN`, `SIGN_KEY`) | The same key. Not with `--docker`, whose container has no keyring; its checksums are signed on the host |
| macOS `.app` and `.dmg` | Ad hoc (`signingIdentity: "-"`) without keys. With them, Developer ID signing and notarisation through an App Store Connect API key | `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_ISSUER`, `APPLE_API_KEY` (key id), `APPLE_API_KEY_P8` (the key file's text; locally `APPLE_API_KEY_PATH`) |
| Windows `.msi` and setup `.exe` | SignPath signs them in CI after the build, outside pull requests; the checksums are redone afterwards. The artifact configuration is `tools/package/signpath-windows-installers.xml` (slug `windows-installers`). The app inside the installer is not signed | Secret `SIGNPATH_API_TOKEN`; variables `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG` |
| Provenance, every package | A GitHub build provenance attestation (Sigstore), outside pull requests | None (the workflow's own token) |

Checking a download:

```bash
sha256sum -c SHA256SUMS-linux.txt                       # macOS: shasum -a 256 -c
gpg --verify SHA256SUMS-linux.txt.asc SHA256SUMS-linux.txt
gh attestation verify OpenMixer_0.1.0_amd64.deb --repo massive-development/openmixer
```

- An ad hoc signed app is still blocked by Gatekeeper on first start: open it once, then choose **Open Anyway** in System Settings, Privacy & Security.
- Until the installers carry a signature with reputation, Windows SmartScreen shows "Windows protected your PC": **More info**, then **Run anyway**.

## Flags

| Flag | Values | Notes and errors |
|---|---|---|
| `--ui=tauri` | | Needs a build with `--features gui-tauri` (else: "--ui=tauri needs the gui-tauri feature"). Implied by the installers' `tauri-default` feature. Ignored with `--headless`. |
| `--ui-bench=SECONDS` | 1 to 600 | Records the lanes' drawn frames for that long after a 3 s warm-up from the first snapshot, writes the report and exits 0. The file is `OPENMIXER_BENCH_OUT` when set and not empty, else `ui-bench.json` in the scratch folder. Refused with `--smoke`, without `--ui=tauri`, or out of range ("--ui-bench needs a whole number from 1 to 600"). |
| `--ui-fps-cap=N` | 0 to 1000, default 60 | 0 is uncapped. Needs `--ui=tauri`. |
| `--debug-panel` | | Shows the frame counter in the top bar (`run_config().hud`); a bench run shows it too. GPUI's flag of the same name shows its device and latency panel. |
| `--ui-drive-out=PATH` | a file in a folder that exists | For `tools/ui-drive.py`: the front end sends its layout map once the layout settles (and after a resize) and the shell writes it to `PATH`; the control thread rewrites the engine's state to `PATH.state` at 10 Hz. Needs `--ui=tauri`. Refused without a path, for a folder, or when the folder is missing. Nothing is written without it. |
| `--smoke` | | Exits 0 ("openmixer tauri smoke ok") once the front end has received a valid packet, painted and called `ready`. With a demo library the front end first waits for deck A's track and sends `deck0.play`; the run fails unless the engine then reports deck A playing ("openmixer tauri control round trip ok"). Fails after 60 s without `ready`. |

## Driving the window

`tools/ui-drive.py` presses, drags, scrolls and types into the running window with real X input and checks every interaction against the engine's state. Linux only (X11, or WSLg's X server), Python 3 standard library only (`libX11` and `libXtst` through `ctypes`, plus `xwininfo` and `xprop`); it never builds.

```bash
npm run build --prefix crates/om-ui-tauri/web && cargo build -p openmixer --features gui-tauri
python3 tools/ui-drive.py                 # the whole suite, about 2.5 minutes
python3 tools/ui-drive.py --only knobs    # one group: buttons, knobs, faders, tempo, hotcues, loops,
                                          # overview, lanes, wheel, keys, disabled
python3 tools/ui-drive.py --verbose       # also the values read during slow drags
```

- It starts `OPENMIXER_BIN` (default `target/debug/openmixer`) with `--ui=tauri --mock-device --ui-drive-out=<temp>/drive.json`, `WAYLAND_DISPLAY=` and its own preferences file, first with `--demo-library` (it waits until both decks report a length and a beat grid), then with no demo flag for the disabled checks. It refuses to start while another `openmixer` runs, and stops the app it started by pid, always.
- **Where things are.** Every control carries its key in `data-control` (`deck0.play`, `deck1.hotcue3`, `deck1.loop.4`, `ch1.eq.low`, `mixer.crossfader`, a fader's `<key>.thumb`, `deck0.overview`, `deck0.lane`). `lib/drive.ts` measures them once the first snapshot has arrived, the fonts have loaded and two frames have painted, and again 250 ms after a resize; nothing runs per frame. The shell's `ui_layout` refuses a map with a missing or extra field, a number that is not finite, a negative size or an empty key, and writes `{ devicePixelRatio, viewport: { width, height }, controls: { key: { x, y, width, height } }, specs }`, rectangles in CSS px from the content area's top left and `specs` the `controls` command's specs, so the script takes ranges, defaults, detents, snap widths and laws from the shell. The window's screen origin comes from `xwininfo -root -tree` (the window whose `_NET_WM_PID` is the app's).
- **What happened.** `PATH.state` is `{ built, decks: [{ loaded, playing, sync, looping, position_frames, length_frames, file_sample_rate, rate, loop_start, loop_end, hotcues, bpm }], channels: [{ fader, trim, filter, eq }], crossfader, master_gain }`, `bpm` the deck's beat grid tempo (null without a grid), written through a temporary file and a rename.
- **What it checks**, one line each (`PASS|FAIL  name  expected  actual`), then `SUMMARY  passed/total`; exit code 1 when a check failed or the run aborted, 2 when it did not start:

  | Group | Checks |
  |---|---|
  | buttons | Play starts and stops each deck; Sync on and off; Cue on a playing deck stops it at frame 0 |
  | knobs | All eleven knobs: double-click restores the default; 80 px up turns half the travel and 80 px down turns it back (within 5% of travel); with Shift held at the press the same 80 px turns a twentieth (a tenth as far); a slow drag across the detent reads only the detent inside the snap width, with Shift it reads values off it (trim, EQ and filter; the master has no pull) |
  | faders | Both channel faders and the crossfader: a click at 25%, 50% and 100% of the track gives the law's value there (the channel faders' taper from `specs`), within 1.5 px; a drag follows mid-drag and at the release; the value holds for 500 ms after the release; double-click restores the default |
  | tempo | Dragging up raises the rate inside 0.92 to 1.08; double-click does not reset it |
  | hotcues | On an empty pad 3 a click sets the cue at the playhead; a second click jumps to it; Shift-click clears it |
  | loops | Loop 4 while playing loops four beats of the grid (within 1%); a click on the lit size ends it |
  | overview | A click at 50% seeks to half the track (within 1%); a drag follows |
  | lanes | 200 px left moves 200 px of time forward (8 s across the lane, within 5%) and holds for 1 s |
  | wheel | Three notches up zoom in by 1.15³ (the next 200 px scrub moves that much less); three down restore it |
  | keys | A knob and a fader: Arrow, Shift+Arrow, Home, End, Delete; a lane (tabbed to from its overview): Arrow (one beat), Shift+Arrow (four), Home, End, `+`, `-`, `0`; Space on Play and on a lit loop; Delete on a set pad |
  | disabled | With empty decks, clicks on Play, Cue, a pad and a loop change nothing and the app keeps running |

- **Safety.** Before every press it checks that the pointer is where it was sent and that the app's window is under it (an override-redirect window such as a drag-and-drop icon counts as foreign), and aborts otherwise. Every button and key it pressed is released in a `finally` block. If another input moves the pointer during a group (a hand on the mouse), that group fails as interrupted and the run goes on.
- **Limits.** It moves the real pointer and types on `DISPLAY`. Under WSLg the window manager gives an X window the keyboard focus only while its Windows twin is the active Windows window and takes back a focus a client sets, so the script sets the focus in the same request batch as each key's events; a key that changed nothing is sent once more (a button's Space acts on its release, which can come after the page lost the focus again), and the summary counts those resends. Tooltips and visual state (thumb place, lit buttons, rings) are not read: it checks the engine, and a thumb that jumps back would show as a value that moves after the release. It drives one window size (1600 × 780) at the scale the display gives; it is not run in CI (no display there).

## Measuring

```bash
tools/ui-bench.sh <gpui|tauri> <seconds> [extra openmixer args...]
tools/ui-bench.sh tauri 20 --demo-library --ui-fps-cap=60
tools/ui-bench.sh gpui 20 --demo-library
tools/ui-bench.sh --self-test
```

Linux only; it never builds. It starts `OPENMIXER_BIN` (default `target/debug/openmixer`) with `WAYLAND_DISPLAY=` and `--mock-device` plus the extra arguments (`tauri` adds `--ui=tauri --ui-bench=<seconds>`), waits 5 s, then samples user and system CPU time and resident memory over the app and all its descendants (the WebKit helpers) once a second for `<seconds>`, and stops the app by pid on exit. It prints one line of JSON:

| Field | Meaning |
|---|---|
| `ui`, `seconds` | The window and the requested length |
| `sampledSeconds` | Wall time between the first and last sample |
| `cpuPercentOneCore` | CPU time over that wall time, in percent of one core, summed over the process tree |
| `rssMb` | Resident memory of the tree at the last sample, MB |
| `processes` | Processes in the tree at the last sample |
| `benchSeconds`, `fpsCap` | The app's own report (tauri only, merged in from here down) |
| `viewport` | CSS width, CSS height, device pixel ratio |
| `frameMs` | Interval between drawn lane frames: `n`, `mean`, `p50`, `p95`, `p99`, `max` (nearest-rank), ms |
| `workMs` | Work per drawn frame (every frame task), same statistics, ms |
| `fps` | 1000 / mean frame interval; null with no frames |
| `framesOver2xRefresh` | Intervals above twice the median interval |
| `decksPlaying` | Decks playing when the recording ended |

The frame figures count drawn animation callbacks, not presented frames. The only runs so far were checks of this tooling on WSL software rendering; they are not a hardware comparison. Measuring the ADR's targets needs a machine with a working GPU path, a GPUI baseline and a real track (ADR 0009, Targets carried into the migration).

## Tests

| Command | Runs |
|---|---|
| `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-ui-bridge` | Packet layout and golden file, peaks, control keys, specs (law, ticks, snap, readout), knob captions and dispatch, theme CSS; `tests/meter_table.rs` checks `fixtures/ui-bridge/meter-table.txt` against `om_ui::meter::MeterScale`, `tests/fader_law_table.rs` checks `fixtures/ui-bridge/fader-law.txt` against the specs and `om_ui::fader_law::FaderLaw`, `tests/knob_text_table.rs` checks `fixtures/ui-bridge/knob-text.txt` against `caption_text` and the specs' readouts |
| `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-ui-tauri` | Stream, pacer, stream slot, control loop, deck changes, deck info, controls, strip ink, bench report, smoke verdict, the drive layout map and state file; headless |
| `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p openmixer --features gui-tauri` | The flag parsing in `gui_tauri.rs` (with the banner rule and the frame counter flag), and `tests/tauri_window.rs`: window smoke, stream smoke with the demo library, the control round trip and the refusal without `--mock-device`. Needs a display (WSLg, or `xvfb-run -a`) and a built `web/dist` |
| `npm test` in `crates/om-ui-tauri/web` | Vitest over `src/**/*.test.ts`: packet decode and interpolation against the golden packet, the meter table, the fader law table, the knob text table, IPC, bench, the frame counter, controls (law, snap, fine drag, a drag that is no click), keys, names and spoken knob values, overview, pads, Cue's writes, theme, lanes (scrub, lane keys), the drive layout map |
| `python3 tools/ui-drive.py` | Real pointer and key input into a built window, checked against the engine (see Driving the window); needs a display, not run in CI |
| `npm run check` | `svelte-check` with warnings as errors |
| `tools/ui-bench.sh --self-test` | The bench script's arithmetic on a fake `/proc` |

CI runs the front-end steps and the `gui-tauri` tests in the `web` job of `.github/workflows/rust.yml` (Linux, under Xvfb); see [build.md](../guidelines/build.md#tauri-window-web-front-end).

## Known gaps

See [ADR 0009, Known gaps of the slice](../adr/0009-web-front-end-tauri-trial.md#known-gaps-of-the-slice).

## What the later plans add

ADR 0009, Decision: plan 2, the library, sampler, FX, stems, automix bar and loudness; plan 3, settings, the mapping editor, plugin windows, media keys, OS file drop and shortcuts; plan 4, test tooling parity, the cut-over, removing `om-ui-gpui`, and installers.
