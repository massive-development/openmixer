# ADR 0009: Trial a web front end (Tauri v2) beside GPUI

## Status

Proposed (trial)

Proposed on 2026-11-15. The trial is carried out by the plan [`docs/superpowers/plans/2026-11-15-tauri-frontend-slice.md`](../superpowers/plans/2026-11-15-tauri-frontend-slice.md) (the first of four plans); its last task applies the exit criteria below and sets this ADR to `Accepted` or `Rejected`. Until then ADR 0004's UI choice (GPUI) stands, and GPUI stays the default window.

## Context

The GPUI window needs hand-rolled controls and layout. Knobs, faders, meters, the waveform, the platter and the sampler pads are custom-painted in `om-ui-gpui`'s paint modules (`knob_paint`, `fader_paint`, `meter_paint`, `waveform_paint` and others), and their geometry lives in `om-ui`'s geometry modules (`geometry`, `header_layout`, `main_layout`). ADR 0004 (update of 2026-11-07) puts the app chrome on `gpui-component` and keeps knobs, faders, meters and waveforms custom-painted; every new control or layout change is written and laid out by hand.

A web front end gets layout, text and controls from the browser engine. A spike (source in `openmixer-tauri-spike/` beside this repo) ran the decks screen in a Tauri v2 window to see whether it can keep up with a DJ display. Its results are the Evidence below.

## Decision

Trial a Tauri v2 window beside the GPUI window, behind the `gui-tauri` feature and the `--ui=tauri` flag. The trial covers the decks screen only (waveforms, transport, mixer, meters) on the real engine, and ends at a measured decision gate.

- **ADR 0004's UI choice stands until the trial ends.** GPUI stays the default; nothing in `om-ui-gpui` is removed during the trial.
- **ADR 0008 (the look) is unchanged.** The web front end carries it through CSS variables generated from the theme files, so the theme tokens stay the single source of colour, size and font.
- **Structure.** A toolkit-free crate, `om-ui-bridge`, turns engine state into wire formats (binary snapshot packet, packed waveform peaks, theme CSS) and turns control messages into `ParamId` dispatches. A thin shell crate, `om-ui-tauri`, owns the window, the Tauri commands and a 30 Hz control-side stream thread. A Svelte front end draws the waveform with one WebGL2 shader and everything else with DOM and CSS transforms.
- **Real-time rules are unchanged.** Nothing in the trial runs on the audio thread. The stream thread reads `DjEngine::snapshot()` only; control ids are parsed with `ParamId::from_str` in the Tauri command handler, on a control thread.

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
- **Batched canvas paths:** 1.4 GB and dropped frames.

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

The trial's measurement task fills these gaps where it can (a GPUI baseline on the same machine, a maximised window) and writes the numbers here.

## Consequences

- Two new crates during the trial: `om-ui-bridge` (no window dependency, tested headless) and `om-ui-tauri` (the window). The binary gains the `gui-tauri` feature and the `--ui=tauri` flag; the default build and window do not change.
- The front end adds a JavaScript toolchain (Svelte 5, TypeScript 5, Vite, Vitest) to the build of the `gui-tauri` feature.
- Theme tokens gain a second consumer (generated CSS variables), so a token change reaches both windows.
- If the trial passes, three more plans follow: library, sampler, FX, stems, automix bar and loudness; settings, mapping editor, plugin windows, media keys, OS file drop and shortcuts; then test tooling parity, cut-over, deletion of `om-ui-gpui` and installers. ADR 0004's UI choice would then be superseded.
- If the trial fails, `crates/om-ui-tauri` and the `gui-tauri` feature are removed; `om-ui-bridge` stays only if something else uses it.
- Risk: the spike's figures are from one Windows machine with the limits listed above; the decision rests on the trial's own measurements, not on the spike.

## Exit criteria

Measured on one machine with a working GPU path, 20 s runs, with a real track of at least four minutes. The trial passes when all of these hold:

1. No frame interval above twice the display's refresh period in a 20 s playing run.
2. Mean per-frame work at or below 0.5 ms.
3. Paused CPU at or below 15% of one core.
4. Playing CPU at a 60 fps cap no more than 15 points above the GPUI baseline on the same machine.
5. Waveform visible within 500 ms of the analysis result being available.
6. Scrubbing, zooming and every control in the slice work by hand on Windows.

## References

- Plan: [`docs/superpowers/plans/2026-11-15-tauri-frontend-slice.md`](../superpowers/plans/2026-11-15-tauri-frontend-slice.md)
- Spike source: `openmixer-tauri-spike/` beside this repo
- ADR 0004 (Rust with GPUI), ADR 0008 (the Machined Faceplate), ADR 0005 (experience-first product bar)
