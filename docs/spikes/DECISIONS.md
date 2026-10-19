# Spike decisions

Date: 2026-10-18. Source documents: `docs/spikes/gpui.md`, `docs/spikes/cpal.md`, `docs/spikes/stretch.md` (read those for method, caveats and raw numbers). This record claims nothing they do not support. All spike code is throwaway and lives in `spikes/`. The GPUI and stretch spikes played no audio. Early cpal runs (5 s and 60 s on WSL) were audible: a 440 Hz tone at gain 0.5, before the silent-only rule. Everything since is silent by default; an audible tone is opt-in (`OPENMIXER_AUDIBLE=1`, gain at most 0.05) and user-run only.

**Status: the engine, analysis, library, MIDI and plugins are verified headlessly on Linux, and the GPUI app runs on Linux. The UI (Wave 5) is partial, and the GPUI window has never been viewed by eye. Windows and macOS support is later. Next: the Before-Wave-7 list in `TODO.md`, then Wave 7.**

## Linux first

Decided by the user, with the time-stretch choice made by the author.

- **Scope.** Linux only for now. Windows and macOS support comes later. CI keeps building and testing the Rust workspace on Linux, macOS and Windows so portability does not rot, but no Windows or macOS spike run, device run or listening test is a gate for now.
- **GPUI gate: closed on Linux.** `gpui =0.2.2` ran on WSL2 X11 (Spike A). The only remaining Linux check is a native-Linux run on a real GPU: nice-to-have, not blocking. The fallback toolkit (iced, then Slint) stays only for a Linux failure. Windows and macOS GPUI checks come with Windows and macOS support.
- **Real-device audio: Linux (ALSA/PipeWire) only for now.** Windows WASAPI and macOS CoreAudio come later. The headphone-cue second-stream failure on WSLg stays recorded as an open Linux item, to verify on native Linux with a real device.
- **Time-stretch: decided (author call; the user may revisit by ear).** Rubber Band R3 via FFI in a dedicated `om-stretch` crate, the one crate allowed `unsafe`, behind a small `Stretcher` trait so Signalsmith or a pure-Rust engine can be swapped in later. Rationale: reference quality; GPL-2.0-or-later is AGPLv3-compatible and design-accepted; it was used by the earlier C++ build; stereo CPU is 4.9-5.6% of a core per deck on WSL wall-clock, which is marginal and is to be re-measured in the real engine. The `GPL-2.0-or-later` allow-entry in `deny.toml` is enabled (om-stretch has landed; the reason is recorded in `deny.toml`). The Windows and macOS build cost of the FFI route is a concern for that later work.
- **General rule.** For well-established crates (cpal, symphonia, rusqlite, midir and similar), adopt directly. Reserve spikes for genuine unknowns.

## Summary

| Spike | What was run | Result | Verdict | What the user must do |
|---|---|---|---|---|
| A. GPUI window | `gpui =0.2.2` window with a custom-painted scrolling waveform, built and run on WSL2 (Ubuntu 24.04, X11 via WSLg) | Builds (after a `libc` pin and a missing `libxkbcommon-x11` dev symlink). X11: 60.0 fps release, 30.0 fps debug, about 114% CPU. Wayland (WSLg) panics at startup. Window not visually seen; GPU backend not confirmed, probably software (llvmpipe) | GO on Linux (WSL2 X11). Gate CLOSED on Linux | Nothing blocking. Optional: a native-Linux run on a real GPU, and `vulkaninfo --summary` plus a look at the window on WSL. Windows and macOS runs come later |
| B. cpal output callback | Alloc-free render under `assert_no_alloc` (test, no device); 60 s real stream via the cpal PulseAudio host to WSLg `RDPSink`; second-stream (cue) probe | Render path proven allocation-free. 60 s at a 256-frame request: 0 xruns, 1 late callback, mean gap 6.045 ms, max 15.4 ms (virtualised sink, not hardware-representative). Second stream failed to open on WSLg ("Invalid argument", cause not isolated) | cpal stays the choice. GO on Linux for the single stream (WSL2 pulse; native-Linux device run is an open Linux item). The headphone-cue second stream is an open Linux item | On native Linux with a real device, test `--cue` with two physical output devices. Windows and macOS come later |
| C. Time-stretch | Offline harness comparing `wsola`, `timestretch` (3 profiles), `signalsmith-stretch` and Rubber Band 4.0.0 (R2, R3) on synthetic chord and drums material (WAV in, WAV out) | No pure-Rust candidate passes the design bar in stereo. Signalsmith passes every measured bar; Rubber Band R3 is marginal on CPU | DECIDED: Rubber Band R3 via FFI behind a `Stretcher` trait (author call; see Rescope) | Optional: A/B listen to `spikes/stretch/out/` and revisit by ear; re-measure in the real engine. The `om-stretch` crate and its `deny.toml` entry have landed |

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
- Real-device latency and xrun numbers on native Linux (ALSA/PipeWire) are an open Linux item. Windows (WASAPI) and macOS (CoreAudio) come later.
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
| UI toolkit | `gpui =0.2.2` from crates.io, no Zed git pin. Fallback only on a Linux NO-GO: iced, then Slint | Gate CLOSED on Linux (WSL2 X11 ran; native-Linux GPU run optional). Windows and macOS checks come later |
| Audio I/O | cpal for the engine output callback (rationale corrected above). rodio is acceptable for library preview | Chosen. Linux (ALSA/PipeWire) real-device numbers and the headphone-cue second stream are open Linux items; Windows (WASAPI) and macOS (CoreAudio) come later |
| Time-stretch | Rubber Band R3 via FFI in `om-stretch` (the one `unsafe` crate), behind a `Stretcher` trait | DECIDED (author call; the user may revisit by ear). Crate has landed (`om-stretch`) |
| `deny.toml` | The `GPL-2.0-or-later` entry is enabled (Rubber Band R3 in `om-stretch`; the reason is in `deny.toml`) | Enabled. It was commented out until `om-stretch` landed |

Linux follow-up work may start: the GPUI gate is closed on Linux and the time-stretch decision is made.

## GPUI integration (Todo 8b)

Date: 2026-10-18. Crate: `crates/om-ui-gpui` (drawing and event glue only; the models are in `om-ui`).

- **Not a workspace member.** It has its own `[workspace]` table and the root `Cargo.toml` lists it in `workspace.exclude`. `openmixer` depends on it as an optional path dependency behind the `gui` feature, which is off by default. Reasons: GPUI needs system libraries at build time (on Linux `libxkbcommon-x11-dev`, Wayland and fontconfig headers) and a display to run. The CI matrix runs `cargo test --workspace` on Linux, macOS and Windows, and GPUI has only been built on Linux (WSL2 X11). Making it a member would compile GPUI on every runner, including two platforms this box cannot check.
- **Lockfile.** The root `Cargo.lock` carries the GPUI tree because the optional dependency is resolved. `libc` is pinned to 0.2.188 in both lockfiles (`xattr` 0.2.3 fails to build against libc 0.2.191: `ENOATTR`, as in the spike). `cargo deny check` passes (advisories, bans, licences, sources) with the GPUI tree present, so `deny.toml` is unchanged.
- **Build and run (Linux, X11).** `RUSTFLAGS="-L /tmp/libshim"` with `/tmp/libshim/libxkbcommon-x11.so` linked to the system runtime library (the spike's workaround), `WAYLAND_DISPLAY=` to force X11, then `cargo run -p openmixer --features gui -- --mock-device`. `--smoke` renders 120 frames and exits 0. `--debug-panel` adds the device, latency and parameter column. Without `--mock-device` the binary opens the cpal output (output only) and falls back to the silent mock device with a visible banner when no device opens.
- **CI.** The `gui` feature is built by a separate `gui-check` job in `.github/workflows/rust.yml` (ubuntu-latest, 40 minutes, `continue-on-error: true`, so it does not block merges). It installs the GPUI system packages (`libasound2-dev pkg-config libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libfreetype-dev libfontconfig1-dev libwayland-dev libvulkan-dev libssl-dev libclang-dev`), runs `cargo check -p openmixer --features gui`, and runs `cargo test --lib` in `crates/om-ui-gpui`. It was added unverified: the first runs show which packages a clean runner still lacks, and `continue-on-error` comes off once they pass. The main `check` matrix still does not build the GPUI tree.
- **Verified.** `openmixer --mock-device --smoke` (with and without `--debug-panel`) exits 0 after 120 frames under X11. `xwininfo` lists the 1280x800 window while the app runs. The crate's headless tests drive the widget models over a real `DjEngine` through the same string-id router the UI uses (drag mapping, pitch ends, crossfader, knob travel, engine-side changes pulled into the model). Nobody has looked at the window: layout, colours and text were not checked by eye.
- **Library browser (Todo 13b).** The browser is drawn in `crates/om-ui-gpui/src/library_view.rs` over the glue in `library.rs`, which holds no gpui types and is tested by `tests/library_panel.rs`. Drag and drop uses gpui's in-app `on_drag` and `on_drop` on the deck cards (a row drags as `openmixer.track:<id>`). The search box is a focusable card that reads keystrokes (`key_char`), not an input-method field, so IME composition is not supported. The `#[gpui::test]` TestAppContext tests were not added: they need gpui's `test-support` feature and its extra dependencies; the glue tests cover the same logic without a window.
- **Not done.** No keyboard shortcuts exist for the deck or mixer views. The om-ui widget tests that need visual checks remain untested by eye, and so does the library browser. The settings library page drives the library (Todo 29). See `TODO.md` Todos 26, 26a and 27 for what is partial.

## Audio settings and overlay (Todo 29)

Date: 2026-10-18. Covers the audio settings model, the preferences targets and the settings overlay.

- **Model.** `openmixer::audio_settings::OutputSettings` owns the open device and its callback. A change stops the device, takes the callback back, opens the new setup and restarts the callback; a failed change reopens the previous setup and returns the error. Every output of the device is opened (up to 8), so the pair choice is routing. `OutputBackend` has two implementations: `CpalBackend` (the binary) and `MockBackend` (tests and `--mock-device`). Listing devices opens nothing.
- **Routing across devices.** One `OutputPairs` handle lives in the model and every device is opened through it (`CpalAudioDevice::open_with_pairs`), so master and headphone pairs survive a device change.
- **Restore.** The device state is a JSON string (host, device, rate, buffer, channels) in `AudioPrefs::device_state`. Start-up restores it, else opens the default output. `AppTargetsState` keeps what the targets remember across calls (last map, wanted theme, plugin folder, last audio state); the composition owns it, because the targets object lives for the whole app.
- **Tests.** Every new test runs on `MockBackend` (no device opened). The device tests that touch the default cpal output are `#[ignore]`d: run `OPENMIXER_DEVICE_TESTS=1 cargo test -p om-audio --test cpal_device -- --ignored`.
- **Overlay.** `om-ui-gpui::settings_view`. Opened by the Settings button in a header strip above the layout; closed by Close or a click on the scrim. Audio and appearance are live (the theme switches at once in the window). Every page drives its service (see "Settings pages over the services" below). Escape closes the overlay: the card holds focus while it is open.
- **Build notes.** The om-ui-gpui test binary links only with the `libxkbcommon-x11` shim above (`RUSTFLAGS="-L /tmp/libshim"`). `--mock-device --smoke` exits 0 with that build. The overlay has not been looked at by eye.
- **Not done.** `--settings` start flag; the example-plugin cases of the preferences targets tests wait on Todo 14.

## Settings pages over the services (Todo 29)

Date: 2026-10-19. Crates: `om-ui::settings` (the page models), `om-ui-gpui::settings_view` (the overlay), `openmixer::gui` (the wiring).

- **MIDI.** The page reads the input list from `MidiService` (refreshed when the overlay opens and on each page change, not per frame). Enable and per-input toggles go through the preferences; the applier opens inputs. Map file: a typed path is loaded first and stored only when it loads; the Load button reloads the map in use. Learn targets are full router ids (`deck.<i>.play|rate`, `mixer.ch<i>.fader|trim`, `mixer.crossfader`, `mixer.master`). A learn that finishes shows as a change of the map generation.
- **Library.** `HubLibrary` wraps the window's shared hub. Without a library the scan settings are still stored, and the watch-folder, rescan and import actions say so.
- **Plugins.** The built-in catalog is `openmixer::plugins::builtin_catalog()` (the gain example, linked in as a dependency). Built-ins are not loaded at start-up. Loading or unloading a plugin re-syncs its themes into the window's registry (`sync_plugin_themes`), so they appear in Appearance.
- **Text field.** GPUI has no text input in this overlay, so a path is typed into a small field at the foot of the card: Enter applies, Escape cancels. This covers watch folders, the XML file and the map file.
- **Smoke.** `--mock-device` and `--smoke` attach a mock controller (`Mock controller`); no run in those modes opens hardware. The smoke run prints `MIDI:` and `plugins:` status lines before the ok line.

## MIDI device layer (Todo 32)

Date: 2026-10-19. Crate: `crates/om-midi` (`backend.rs`, `alsa_backend.rs`, `mock.rs`).

- **Why not midir yet.** midir's Linux backend links `alsa` at a version that conflicts with the one cpal links, and its macOS backend pins CoreMIDI crates that clash with the gpui tree. Adopting it in the workspace now would break the build on one platform or the other.
- **Linux: ALSA sequencer, direct.** `AlsaBackend` uses the `alsa` crate at the version already in `Cargo.lock` through cpal (`alsa = "0.11"`), so the build links one `alsa`. Inputs and outputs are sequencer clients subscribed to the device's ports. Port ids are the port names, the same strings the MIDI settings page shows. The backend has no unsafe code of its own (the `alsa` crate carries the FFI); construction and enumeration are smoke-tested and skip when `/dev/snd/seq` is absent.
- **Other platforms: documented stub.** `om_midi::system_backend()` returns a device error outside Linux, so the app runs without MIDI there. The midir backend for macOS and Windows plugs in at that point: it implements `MidiBackend` (enumerate, open input with a byte callback, open output, `refresh` for hot-plug) and needs no change to the routers, the mapper or the feedback unit.
- **Tests never open hardware.** `MockBackend` delivers injected bytes and records sent bytes. Every routing, selection, hot-plug and feedback test runs on it.
- **Trait boundary.** `MidiBackend` is the only thing the device-facing code knows. Input callbacks run on the backend's thread and take only the mapper's locks; feedback is evaluated on a control thread at 30 Hz, and hot-plug is checked about once a second.


## Plugins: in-process only (Todo 10a)

Date: 2026-10-19. Crate: `crates/om-plugins`. Example: `examples/plugins/gain-plugin`.

- **Decision: plugins are in-process for now.** A plugin is a Rust type that implements `Plugin`, registered with `PluginRegistry::add_plugin` (built-ins, tests, or a crate the application links). The registry enforces these rules: unique ids, refused duplicate stage, map, prefix and theme names, and unload that removes everything a plugin registered before its `on_unload`.
- **Dynamic loading is a later decision, not made here.** Rust has no stable ABI, so a shared library built from Rust cannot be loaded safely by an unrelated build. The options are: a C ABI (`extern "C"` entry points and a `#[repr(C)]` function table, versioned), a WASM component (sandboxed and language-neutral, with a runtime cost), or source-level plugins only. `PluginLoader` in `om-plugins` is the stub: it finds shared libraries in a folder and reports each as "not supported", so the Plugins page shows what it skipped. No allow-list or signature check is planned (CONTRIBUTING, "Extensibility").
- **Design note.** `Plugin::info` and `Plugin::api_version` take `&self`. Associated functions with no receiver would make the trait unusable as `Box<dyn Plugin>`, which the registry needs.
- **Stage insertion goes through `InsertPoint`.** `MixerControl` and `DjEngine` implement it, so the catalog can be tested over a stub and a plugin stage can always be cleared by the handle it was inserted through.

## Stem separation: downloads and backend (Todo 19)

Date: 2026-10-19. Crate: `crates/om-stems`.

- **Downloads.** Weights download over HTTP, HTTPS and file URLs. The implementation keeps the licence gate, the `.part` file, the size and SHA-256 checks and the marker files. Transport: `file://`, `http://` and `https://` (`ureq` 2 with rustls). Tests use local files and a loopback server; no test reaches the network.
- **HTTPS uses rustls with the webpki root bundle.** `webpki-roots` is CDLA-Permissive-2.0 (a permissive data licence, compatible with AGPL-3.0 distribution). `deny.toml` allows it with a comment. The https test reaches a loopback listener that hangs up and checks the TLS handshake fails, so no test leaves the host.
- **The ONNX Runtime backend stays open.** `StemSeparator` is the interface; `MockSeparator` stands in and returns four quarters of the input, so the pipeline can run without a model. The `ort` backend, model export and the hosted manifest are Todo 19 work.

## Repository layout after the move to Rust

Date: 2026-10-19. The Rust workspace is the whole project. The C++ sources, CMake build and their workflows were removed; ADR 0004 records the move.

- **Kept:** `resources/themes/` (read by `om-ui`), `LICENSE`, `NOTICE`, `CODE_OF_CONDUCT.md`, and the test data in the repo-root `fixtures/`.
- **CI:** the progress heatmap check (`progress-check`, which `tools/progress_heatmap.py --check` backs) and the ASIO SDK guard (`legal-guards`: a `git grep` for `asiodrv` and `ASIO SDK` over every tracked file outside `docs/`, `.github/`, `*.md` and `NOTICE`, which name the rule; then a file-name check on tracked paths for `iasiodrv`, `asiosdk` and `asiodrivers`).
- **Licence check:** `cargo deny` (the `deny` job) covers the Rust dependency licences. Signing is still a pre-release decision.
- **Gitignore:** the old C++ build outputs (`/build/`, `/build-*/`, `/_deps/`, `/CMakeFiles/`) are ignored via `.gitignore`.
- **Not verified:** the `gui-check` job (see the GPUI integration section above) has not yet run on a clean runner.

## CI status by platform

Date: 2026-10-19. Run 39296306022 on PR #66. Linux is the required signal; macOS and Windows legs are `continue-on-error` in `rust.yml` until Windows and macOS support lands.

- **ubuntu-latest:** green (fmt, clippy, test, `cargo deny`, legal guards, progress heatmap).
- **windows-latest:** `om-library` `file_url` tests asserted `/` separators, but `native_separators` returns `\` on Windows by design. Fixed in the test (expectations go through a platform helper); the fix is not yet confirmed by a CI run. fmt, clippy and the other test binaries already passed on this leg, so no other Windows work is known.
- **macos-latest:** `om-audio` `soak_short` failed on its xrun budget (23 xruns against a budget of 5, max callback 38.7 ms against 5.3 ms). The test is timing-based and the hosted runner is shared, so the budget is not loosened. Later (Windows and macOS support): run the soak on a dedicated macOS CoreAudio device and set the budget per platform from measured numbers.
- **Build deps:** `rubberband-sys` (bindgen) needs `libclang`. The workflow installs `libclang-dev` only on Linux. The macOS and Windows legs compiled without a failure in the build step, so no LLVM install was needed in this run. Later (Windows and macOS support): keep an eye on bindgen when the Windows toolchain or the audio backends change.
- **GUI:** the `gui check` job is Linux-only and still non-blocking; it has not been verified on a clean runner.
