# Phase 1 spike decisions

Date: 2026-10-18. Source documents: `docs/spikes/gpui.md`, `docs/spikes/cpal.md`, `docs/spikes/stretch.md` (read those for method, caveats and raw numbers). This record claims nothing they do not support. All spike code is throwaway and lives in `spikes/`. The GPUI and stretch spikes played no audio. Early cpal runs (5 s and 60 s on WSL) were audible: a 440 Hz tone at gain 0.5, before the silent-only rule. Everything since is silent by default; an audible tone is opt-in (`OPENMIXER_AUDIBLE=1`, gain at most 0.05) and user-run only.

**Status: Phase 0 complete; Phase 1 spikes run — Linux-first; GPUI gate closed on Linux, stretcher decided (Rubber Band R3 behind a Stretcher trait); Windows/macOS deferred to a porting phase.**

## Rescope: Linux-first

Decided by the user, with the time-stretch choice made by the author. This supersedes the earlier "pending the user" wording in this file, in `docs/spikes/*.md`, the design and ADR 0004.

- **Scope.** Phase 2 and later target Linux only. Windows and macOS become a later porting phase. CI keeps building and testing the Rust workspace on Linux, macOS and Windows so portability does not rot, but no Windows or macOS spike run, device run or listening test is a gate any more.
- **GPUI gate: closed on Linux.** `gpui =0.2.2` ran on WSL2 X11 (Spike A). The only remaining Linux check is a native-Linux run on a real GPU: nice-to-have, not blocking. The fallback toolkit text (iced, then Slint) stays only for a Linux failure. Windows and macOS GPUI checks move to the porting phase.
- **Real-device audio: Linux (ALSA/PipeWire) only for now.** Windows WASAPI and macOS CoreAudio move to the porting phase. The headphone-cue second-stream failure on WSLg stays recorded as an open Linux item for Phase 2, to verify on native Linux with a real device.
- **Time-stretch: decided (author call; the user may revisit by ear).** Rubber Band R3 via FFI in a dedicated `om-stretch` crate, the one crate allowed `unsafe`, behind a small `Stretcher` trait so Signalsmith or a pure-Rust engine can be swapped in later. Rationale: reference quality; GPL-2.0-or-later is AGPLv3-compatible and design-accepted; it is used in the C++ baseline; stereo CPU is 4.9-5.6% of a core per deck on WSL wall-clock, which is marginal and is to be re-measured in the real engine. The `GPL-2.0-or-later` allow-entry in `deny.toml` must be enabled when `om-stretch` lands in Phase 2. It is not enabled now. The Windows and macOS C++ build cost of the FFI route is a porting-phase concern.
- **General rule.** For well-established crates (cpal, symphonia, rusqlite, midir and similar), adopt directly. Reserve spikes for genuine unknowns.

## Summary

| Spike | What was run | Result | Verdict | What the user must do |
|---|---|---|---|---|
| A. GPUI window | `gpui =0.2.2` window with a custom-painted scrolling waveform, built and run on WSL2 (Ubuntu 24.04, X11 via WSLg) | Builds (after a `libc` pin and a missing `libxkbcommon-x11` dev symlink). X11: 60.0 fps release, 30.0 fps debug, about 114% CPU. Wayland (WSLg) panics at startup. Window not visually seen; GPU backend not confirmed, probably software (llvmpipe) | GO on Linux (WSL2 X11). Gate CLOSED on Linux | Nothing blocking. Optional: a native-Linux run on a real GPU, and `vulkaninfo --summary` plus a look at the window on WSL. Windows and macOS runs moved to the porting phase |
| B. cpal output callback | Alloc-free render under `assert_no_alloc` (test, no device); 60 s real stream via the cpal PulseAudio host to WSLg `RDPSink`; second-stream (cue) probe | Render path proven allocation-free. 60 s at a 256-frame request: 0 xruns, 1 late callback, mean gap 6.045 ms, max 15.4 ms (virtualised sink, not hardware-representative). Second stream failed to open on WSLg ("Invalid argument", cause not isolated) | cpal stays the choice. GO on Linux for the single stream (WSL2 pulse; native-Linux device run is a Phase 2 item). The headphone-cue second stream is an open Linux item for Phase 2 | On native Linux with a real device, test `--cue` with two physical output devices. Windows and macOS moved to the porting phase |
| C. Time-stretch | Offline harness comparing `wsola`, `timestretch` (3 profiles), `signalsmith-stretch` and Rubber Band 4.0.0 (R2, R3) on synthetic chord and drums material (WAV in, WAV out) | No pure-Rust candidate passes the design bar in stereo. Signalsmith passes every measured bar; Rubber Band R3 is marginal on CPU | DECIDED: Rubber Band R3 via FFI behind a `Stretcher` trait (author call; see Rescope) | Optional: A/B listen to `spikes/stretch/out/` and revisit by ear; re-measure in the real engine. The `om-stretch` crate and its `deny.toml` entry land in Phase 2 |

## Spike A: GPUI window (go/no-go gate)

- GPUI is available on crates.io as `gpui` 0.2.2 (Apache-2.0). It is pinned with `gpui = "=0.2.2"`; no Zed git pin is needed, which supersedes the earlier design note that found no authoritative crates.io status.
- WSL2 X11: runs at 60.0 fps in release (30.0 fps debug, not understood), about 114% CPU. The window was not visually seen (non-interactive session). `vulkaninfo` was not installed. There is no `dzn` ICD and no `/dev/dri`, so the device is most likely llvmpipe (software), unconfirmed.
- WSL2 Wayland (WSLg): panic `UnsupportedVersion` in GPUI's Wayland client; `WAYLAND_DISPLAY=` forces X11. Per the design this is a dev-loop issue and does not fail the gate on its own.
- Build friction: a fresh dependency resolve breaks (`libc` 0.2.191 vs `xattr`); the committed `Cargo.lock` pins `libc` 0.2.188. Needs `libxkbcommon-x11-dev` on Linux. Budget for dependency churn.
- Windows and macOS were not run and are not gates any more (Linux-first, see Rescope). The gate is closed on Linux. A Linux failure would trigger the fallback: a toolkit switch, iced first, then Slint (not dropping the platform).

## Spike B: cpal output callback

- The render path (`render(out, channels, state, cmd_consumer)` driven by an `rtrb` SPSC queue) is allocation-free under `assert_no_alloc`; a mutation check confirmed the guard works.
- Real stream on WSLg through the cpal PulseAudio host: `BufferSize::Fixed(256)` accepted, but the delivered callback size and rate are not confirmed to be exactly 256 frames (mean gap 4% above nominal). No output latency was measured; this is callback-timing jitter only. The ALSA host has no device on this WSL box (build-only). The 60 s run was made before the silent-by-default rule and was audible; a later 4 s silent re-run gave comparable timing.
- Second stream (headphone cue): both attempts failed with "Invalid argument" while the first stream was playing, on the only device available. The cause (cpal Pulse host, WSLg sink, or the probe) was not isolated. Two simultaneous streams are therefore unproven on every backend; this is the main risk carried forward.
- Real-device latency and xrun numbers on native Linux (ALSA/PipeWire) are a Phase 2 item. Windows (WASAPI) and macOS (CoreAudio) moved to the porting phase.
- rodio correction: the earlier ADR/design wording said rodio cannot control buffer size. That is wrong. rodio 0.23 has `DeviceSinkBuilder::with_buffer_size`. The case for cpal is owning the render callback (allocation-free, SPSC-driven, no iterator-per-sample interface or hidden mixer thread) and opening multiple streams for the headphone cue, not buffer control. Whether rodio can open two devices at once was not checked. rodio remains acceptable for library preview/audition.

## Spike C: time-stretch (keylock)

Design bar: pitch error under 5 cents, and one deck under 5% of a core, judged on stereo. The stereo test material is L plus R = a 0.9x copy of L delayed 331 samples (correlated at 7.5 ms). Measured on synthetic material only, wall-clock on a shared WSL2 VM (plus or minus 20%). Nothing has been heard by an agent.

Stereo CPU per deck: signalsmith 0.8-1.0%; `timestretch` Keylock/batch 2.1-3.1% (but fails pitch at 0.92 and transients); Rubber Band R3 4.9-5.6% (marginal); `timestretch` WideKeylock about 5.0% (4.98% at 0.92, 5.71% at 1.08; transients poor); `wsola` 5.3-6.2% (fails the CPU bar; pitch and transients good).

Findings:
- **No pure-Rust candidate passes the design bar in stereo.** `wsola` and `timestretch-wide` keep pitch but are at or over 5%; the cheap `timestretch` Keylock fails pitch. `wsola` is an unoptimised brute-force search, so tuning might bring it under 5%, but that is untested, and four decks need margin well under 5% each.
- Signalsmith (C++ via `cc`, MIT) passes every measured bar: pitch under 1 cent, attack rise 3.7-7.2 ms (original 2.2 ms), cheapest CPU. It is FFI, not pure Rust. Only its Linux build was tried.
- Rubber Band R3 (`ENGINE_FINER`) is the trustworthy Rubber Band reference: pitch within 3.4 cents on the strict estimator, attack rise 5.4-7.1 ms. R2 (default options) read about 20 cents off on a steady sine; cause not root-caused (a flaw in the vendored build cannot be ruled out), so R2 numbers are not evidence.
- Rubber Band's GPL-2.0-or-later is AGPLv3-compatible, already used in the C++ build, and accepted by the design, so licence is not an argument against it.
- Caveats: synthetic mono material for pitch and transients; no real music, no stereo pitch/transient metrics, no callback worst-case timing, no live tempo-ramp test.

### Listening files (optional)

The A/B files are generated locally and are **not committed** (`spikes/stretch/out/` is gitignored). Regenerate them offline, which opens no audio device and plays nothing:

```
cd spikes/stretch && cargo run --release -- eval
```

This writes about 42 WAVs (2 originals plus 40 stretched) and `results.md` into `spikes/stretch/out/`, in roughly 35 s. Then listen to them by hand. See `docs/spikes/stretch.md` ("Listening checklist") for what to listen for.

### Options (historical; option B chosen)

- **A. Signalsmith via a thin FFI crate.** Cheapest CPU and passes every measured bar. Costs: not pure Rust, so FFI becomes the default path instead of the fallback; needs a C++ toolchain on every OS (MSVC, Xcode tools); Windows/macOS build cost unmeasured. MIT, so no `deny.toml` change.
- **B. Rubber Band R3 via FFI.** Reference quality. GPL-2.0-or-later is AGPLv3-compatible and design-accepted. Marginal CPU (4.9-5.6% stereo per deck, so four decks would be tight). Same C++ build cost, plus `bindgen`/libclang through `rubberband-sys`. Needs the `GPL-2.0-or-later` entry in `deny.toml`.
- **C. Keep looking for or tune a pure-Rust stretcher.** Keeps the design's pure-Rust-first stance. `wsola` tuning (smaller search range, larger hop, own optimisation) is untested and may or may not get under the bar; whether it sounds good enough is a question for ears.

**Author's recommendation at the time (superseded by the Rescope decision, option B):** after listening, option A if Signalsmith sounds acceptable (it is the only candidate that passed every measured bar with CPU margin), with B as the quality reference to compare against; choose C only if the listening test shows `wsola` is good enough to justify an optimisation effort. This is the spike author's reading of the numbers, made without hearing any output.

## Decisions

| Area | Decision | Status |
|---|---|---|
| UI toolkit | `gpui =0.2.2` from crates.io, no Zed git pin. Fallback only on a Linux NO-GO: iced, then Slint | Gate CLOSED on Linux (WSL2 X11 ran; native-Linux GPU run optional). Windows and macOS checks in the porting phase |
| Audio I/O | cpal for the engine output callback (rationale corrected above). rodio is acceptable for library preview | Chosen. Linux (ALSA/PipeWire) real-device numbers and the headphone-cue second stream are open Phase 2 items; Windows (WASAPI) and macOS (CoreAudio) are in the porting phase |
| Time-stretch | Rubber Band R3 via FFI in `om-stretch` (the one `unsafe` crate), behind a `Stretcher` trait | DECIDED (author call; the user may revisit by ear). Crate lands in Phase 2 |
| `deny.toml` | The `GPL-2.0-or-later` entry stays commented out until Phase 2 | Enable it when `om-stretch` lands (option B). Not enabled now: it would allow GPL-2.0-or-later crates workspace-wide before the crate exists |

Phase 2 planning may start: the GPUI gate is closed on Linux and the time-stretch decision is made.

## GPUI integration (Todo 8b)

Date: 2026-10-18. Crate: `crates/om-ui-gpui` (drawing and event glue only; the models are in `om-ui`).

- **Not a workspace member.** It has its own `[workspace]` table and the root `Cargo.toml` lists it in `workspace.exclude`. `openmixer` depends on it as an optional path dependency behind the `gui` feature, which is off by default. Reasons: GPUI needs system libraries at build time (on Linux `libxkbcommon-x11-dev`, Wayland and fontconfig headers) and a display to run. The CI matrix runs `cargo test --workspace` on Linux, macOS and Windows, and GPUI has only been built on Linux (WSL2 X11). Making it a member would compile GPUI on every runner, including two platforms this box cannot check.
- **Lockfile.** The root `Cargo.lock` carries the GPUI tree because the optional dependency is resolved. `libc` is pinned to 0.2.188 in both lockfiles (`xattr` 0.2.3 fails to build against libc 0.2.191: `ENOATTR`, as in the spike). `cargo deny check` passes (advisories, bans, licences, sources) with the GPUI tree present, so `deny.toml` is unchanged.
- **Build and run (Linux, X11).** `RUSTFLAGS="-L /tmp/libshim"` with `/tmp/libshim/libxkbcommon-x11.so` linked to the system runtime library (the spike's workaround), `WAYLAND_DISPLAY=` to force X11, then `cargo run -p openmixer --features gui -- --mock-device`. `--smoke` renders 120 frames and exits 0. `--debug-panel` adds the device, latency and parameter column. Without `--mock-device` the binary opens the cpal output (output only) and falls back to the silent mock device with a visible banner when no device opens.
- **CI.** Unchanged: the `gui` feature is not built in CI. Open item (Todo 29, `.github/workflows/rust.yml` has the TODO): a Linux-only job for `cargo check -p openmixer --features gui`. The link line on this box needs the development packages for freetype, xcb, xkbcommon and xkbcommon-x11; wayland and fontconfig are not verified on a clean runner. Not added: the job would compile the whole GPUI tree on every run, and the apt list cannot be verified here.
- **Verified.** `openmixer --mock-device --smoke` (with and without `--debug-panel`) exits 0 after 120 frames under X11. `xwininfo` lists the 1280x800 window while the app runs. The crate's headless tests drive the widget models over a real `DjEngine` through the same string-id router the UI uses (drag mapping, pitch ends, crossfader, knob travel, engine-side changes pulled into the model). Nobody has looked at the window: layout, colours and text were not checked by eye.
- **Library browser (Todo 13b).** The browser is drawn in `crates/om-ui-gpui/src/library_view.rs` over the glue in `library.rs`, which holds no gpui types and is tested by `tests/library_panel.rs`. Drag and drop uses gpui's in-app `on_drag` and `on_drop` on the deck cards (a row drags as `openmixer.track:<id>`). The search box is a focusable card that reads keystrokes (`key_char`), not an input-method field, so IME composition is not supported. The `#[gpui::test]` TestAppContext tests were not added: they need gpui's `test-support` feature and its extra dependencies; the glue tests cover the same logic without a window.
- **Not done.** No C++ keyboard shortcuts exist for the deck or mixer views. The om-ui widget tests that need visual checks remain untested by eye, and so does the library browser. The gpui settings library page is still a placeholder (Todo 29). See `TODO.md` Todos 26, 26a and 27 for what is partial.

## Audio settings and overlay (Todo 29, 9b)

Date: 2026-10-18. Ports `JuceAudioSettingsModel`, `AppPreferencesTargets` and the settings overlay.

- **Model.** `openmixer::audio_settings::OutputSettings` owns the open device and its callback. A change stops the device, takes the callback back, opens the new setup and restarts the callback; a failed change reopens the previous setup and returns the error. Every output of the device is opened (up to 8), so the pair choice is routing, as in C++. `OutputBackend` has two implementations: `CpalBackend` (the binary) and `MockBackend` (tests and `--mock-device`). Listing devices opens nothing.
- **Routing across devices.** One `OutputPairs` handle lives in the model and every device is opened through it (`CpalAudioDevice::open_with_pairs`), so master and headphone pairs survive a device change.
- **Restore.** The device state is a JSON string (host, device, rate, buffer, channels) in `AudioPrefs::device_state`. Start-up restores it, else opens the default output. `AppTargetsState` keeps what the targets remember across calls (last map, wanted theme, plugin folder, last audio state); the composition owns it, because the C++ targets object lives for the whole app.
- **Tests.** Every new test runs on `MockBackend` (no device opened). The one existing test that opens the default cpal output is now opt-in: set `OPENMIXER_DEVICE_TESTS=1`.
- **Overlay.** `om-ui-gpui::settings_view`. Opened by the Settings button in a header strip above the layout; closed by Close or a click on the scrim. Audio and appearance are live (the theme switches at once in the window). MIDI, library and plugin pages show their stored state and say what is not ported. Escape is not wired: GPUI key events need a focus handle, which is not set up yet.
- **Build notes.** The om-ui-gpui test binary links only with the `libxkbcommon-x11` shim above (`RUSTFLAGS="-L /tmp/libshim"`). `--mock-device --smoke` exits 0 with that build. The overlay has not been looked at by eye.
- **Not done.** `--settings` start flag; plugin themes reach only the composition's registry (the window keeps its own copy, loaded from the built-ins and the user folder); the example-plugin cases of `AppPreferencesTargetsTest` wait on Todo 14.
