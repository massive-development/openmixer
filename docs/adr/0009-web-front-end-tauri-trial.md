# ADR 0009: Move the interface to a web front end (Tauri v2)

## Status

Accepted (2026-11-16)

Amended 2026-11-21: **GPUI removed (2026-11-21).** The window is Tauri only: `om-ui-gpui` and the binary's `gui.rs` were deleted ahead of the porting plans above, by the owner's decision to cut the bloat. The last commit that still has them is `117ea60`; read old code with `git show 117ea60:crates/om-ui-gpui/<path>`. The GPUI-free glue moved to `om-ui` (`library::hub`) and `openmixer::wiring`. Features the Tauri window does not have yet (library, stems, Automix, sampler, recording, media keys) are ported from that code.

Accepted by the project owner's decision after the decks slice was built and reviewed. The exit criteria were not measured; they remain below as targets for the migration, not as a gate.

Proposed on 2026-11-15 as a trial: a decks-screen slice built beside the GPUI window (the first of four planned steps). The trial's measurements were not run: there is no GPUI baseline on real hardware, no run on a machine with a working GPU path under Linux, no maximised-window run, and the pointer interactions have not been exercised by hand.

## Context

The GPUI window needs hand-rolled controls and layout. Knobs, faders, meters, the waveform, the platter and the sampler pads are custom-painted in `om-ui-gpui`'s paint modules (`knob_paint`, `fader_paint`, `meter_paint`, `waveform_paint` and others), and their geometry lives in `om-ui`'s geometry modules (`geometry`, `header_layout`, `main_layout`). ADR 0004 (update of 2026-11-07) puts the app chrome on `gpui-component` and keeps knobs, faders, meters and waveforms custom-painted; every new control or layout change is written and laid out by hand.

A web front end gets layout, text and controls from the browser engine. A spike (source in `openmixer-tauri-spike/` beside this repo) ran the decks screen in a Tauri v2 window to see whether it can keep up with a DJ display. Its results are the Evidence below.

## Decision

The web front end (Tauri v2 with Svelte) is the direction for the interface. It was first built as a trial window beside the GPUI window, behind the `gui-tauri` feature and the `--ui=tauri` flag, for the decks screen only (waveforms, transport, mixer, meters) on the real engine. The trial was to end at a measured gate; it ended instead with the owner's acceptance (see Status).

- **What acceptance means.** The interface moves to the web front end. ADR 0004's UI choice (GPUI) is superseded in direction by this ADR; the changeover is completed by the later plans: plan 2, the library and the remaining panels (sampler, FX, stems, automix bar, loudness); plan 3, settings, the mapping editor, plugin windows, media keys, OS file drop and shortcuts; plan 4, test tooling parity, the cut-over, removal of `om-ui-gpui`, and installers.
- **GPUI stays the default and shipping window until the cut-over plan lands.** The web window stays behind the `gui-tauri` feature and `--ui=tauri`; `om-ui-gpui` is not removed yet. Amended 2026-11-17 by the owner's decision: the web window opens the real output device as GPUI does (the mock stays for `--mock-device`, smoke, bench, drive and demo runs), and installers that ship it (`tools/package.sh`, the `tauri-default` feature) come ahead of the cut-over.
- **ADR 0008 (the look) is unchanged.** The web front end carries it through CSS variables generated from the theme files, so the theme tokens stay the single source of colour, size and font.
- **Structure.** A toolkit-free crate, `om-ui-bridge`, turns engine state into wire formats (binary snapshot packet, packed waveform peaks, theme CSS) and turns control messages into `ParamId` dispatches. A thin shell crate, `om-ui-tauri`, owns the window, the Tauri commands and a 30 Hz control-side stream thread. A Svelte front end draws the waveform with one WebGL2 shader and everything else with DOM and CSS transforms.
- **Real-time rules are unchanged.** Nothing in the trial runs on the audio thread. The stream thread reads `DjEngine::snapshot()` only; control ids are parsed with `ParamId::from_str` in the Tauri `control` command, which runs on the window's main thread (so writes keep their order), never the audio thread.

### Rendering rules adopted

1. Analyse once and upload peaks once, as a texture.
2. One shader draws all waveform lanes.
3. Meters, playheads, bar numbers and cue flags move by CSS transform.
4. Skip the frame when nothing changed.
5. Write text only when it changes.
6. Stream state at 30 Hz and interpolate between packets.
7. Small per-frame packets; bulk data (peaks, theme) is sent once.

### Rejected in the spike

- **Pre-rendered tile scrolling:** the tiles must be rebuilt on every zoom.
- **Batched canvas paths:** 1.4 GB of memory and dropped frames.

## Evidence

From the spike. Windows, window 1600×780, two decks playing. CPU is the percent of one core, summed over the app and its WebView2 processes.

| Run | CPU |
|---|---|
| Uncapped, 120 fps | 43% |
| Capped to 60 fps | 26% |
| Capped to 30 fps | 17% |
| Paused | 13% |
| Snapshot stream at 30 Hz | 35% |
| Snapshot stream at 120 Hz | 49% |

- 4 KB packets add 6 to 11 points of CPU.
- No frame over 20 ms, except one 31 ms frame in the 120 Hz stream run.
- Zero packets lost.

### Limits of the evidence

- Frame figures count animation callbacks, not presented frames.
- GPU load was not measured.
- There is no GPUI baseline; the numbers above are not a comparison with the current window.
- Nothing larger than 1600×780 was run.
- Linux was measured only under WSL software rendering, where `WEBKIT_DISABLE_DMABUF_RENDERER=1` cut CPU from 213% to 56%.

The trial's measurement task was to fill these gaps (a GPUI baseline on the same machine, a maximised window); it was not run before acceptance.

### Evidence from the slice

Only tooling checks of `tools/ui-bench.sh`, run under WSL with software rendering while the slice was built:

- Tauri window: about 65 to 120% of one core with one deck playing, about 8% paused.
- GPUI window: about 599% of one core on the same machine and run.
- Per-frame work in the Tauri window: about 0.22 to 0.25 ms.

These come from a machine with no working GPU path. They are not a hardware comparison and do not test the targets below.

## Consequences

- Two new crates: `om-ui-bridge` (no window dependency, tested headless) and `om-ui-tauri` (the window). The binary gains the `gui-tauri` feature and the `--ui=tauri` flag; the default build and window do not change until the cut-over.
- The front end adds a JavaScript toolchain (Svelte 5, TypeScript 5, Vite, Vitest) to the build of the `gui-tauri` feature.
- Theme tokens gain a second consumer (generated CSS variables), so a token change reaches both windows.
- Three more plans follow: library, sampler, FX, stems, automix bar and loudness; settings, mapping editor, plugin windows, media keys, OS file drop and shortcuts; then test tooling parity, cut-over, deletion of `om-ui-gpui` and installers. ADR 0004's UI choice is superseded in direction now and in fact at the cut-over.
- Risk: the decision was taken without the trial's measurements. The spike's figures are from one Windows machine with the limits listed above, and the slice's figures are tooling checks under software rendering; how the web window performs on real hardware against GPUI is not known yet.

## Targets carried into the migration

These were the trial's exit criteria. They were not measured before acceptance and are kept as targets for the migration, measured on one machine with a working GPU path, 20 s runs, with a real track of at least four minutes:

1. No frame interval above twice the display's refresh period in a 20 s playing run.
2. Mean per-frame work at or below 0.5 ms.
3. Paused CPU at or below 15% of one core.
4. Playing CPU at a 60 fps cap no more than 15 points above the GPUI baseline on the same machine.
5. Waveform visible within 500 ms of the analysis result being available.
6. Scrubbing, zooming and every control in the slice work by hand on Windows.

## Known gaps of the slice

What the trial window does not yet match or prove, so the measurements are read against it:

- **The beat grid on the generated demo tracks sits about half a beat off the kicks.** It is the analysed grid, the same data GPUI draws; it has not been checked against the GPUI window.
- **Pointer and keyboard input are driven by injected X events, not by hand on Windows.** `tools/ui-drive.py` (feature page, "Driving the window") drives every slice interaction with XTest under WSLg and checks it against the engine: knob and fader drags (Shift fine drags, detent pull, double-click reset), pad, loop, Play, Cue and Sync clicks, overview seek, lane scrub, wheel zoom, the slider and lane keys and Space. Under WSLg each key is given the X focus in the same batch as its events, because the window manager withholds the focus from a window whose Windows twin is not active. What it does not read: tooltips and visual state (thumb place, lit buttons, focus rings), only the engine's values. No screen reader has been run against the window.
- **Text at 125% was simulated,** by scaling the type tokens; WebKitGTK's own text zoom and WebView2's text scaling have not been tried.
- **Device choice (partly closed).** The window opens the stored or default output device (amendment of 2026-11-17) and now has a Settings overlay whose Audio page changes the device, rate, buffer, master and cue output. Its MIDI, Mapping and Plugins pages follow; it has no other settings page yet.
- **The mock-device notice points at Settings,** which the web window now has (its Audio page only): it shows GPUI's text for `Notice::Demo` ("Choose an output in Settings."), and `om_ui::labels` has no notice that says the same without it (`Notice::NoDevice` is the failed-device notice).

## References

- Spike source: `openmixer-tauri-spike/` beside this repo
- ADR 0004 (Rust with GPUI; UI choice superseded in direction by this ADR), ADR 0008 (the Machined Faceplate), ADR 0005 (experience-first product bar)
