# OpenMixer Roadmap

This document outlines the plan for OpenMixer. What the product has to be is in `docs/PRODUCT.md`; task-level status is in `openmixer-workbench/TODO.md` and `openmixer-workbench/docs/PROGRESS.md`.

## Tracks

Work runs in parallel tracks, each measured against the product bar. No track waits for another to finish, and Experience is not gated on the engine.

| Track | Outcome a DJ notices | Where it lives today |
|-------|----------------------|----------------------|
| Experience | The screen looks like a product and reads at a glance: deck-coloured waveforms (the Low-Band Rule) with bar numbers and cues, rotary EQ, large labelled pads, a library with artwork and compatibility cues | `openmixer-workbench/TODO.md` "Experience track" |
| Performance | No dropouts over a long set; honest latency; low CPU with four decks and stems | Waves 2 and 9, soak gates |
| Hardware | A controller works when plugged in | Wave 6 |
| Intelligence | Stems, Automix and the sampler save time and sound right | Wave 7 |
| Pro surface | Recording, broadcast, DVS, video | Wave 8 |
| Release | Installable, legal, documented | Wave 9 |

## Waves

The nine implementation waves below are stable identifiers for the tasks in `openmixer-workbench/TODO.md` and `openmixer-workbench/docs/PROGRESS.md`. They are not an order of work.

| Wave | Area | Focus |
|------|-------|-------|
| 1 | Foundation | Repo setup, legal rulebook, Rust workspace, CI, test harness |
| 2 | Audio Engine | Device layer, decks, mixer, MIDI input, debug panel, then time-stretch, sync, FX, performance gate |
| 3 | Analysis | BPM/key detection, waveforms, structure features, stem separation |
| 4 | Library | Database, metadata import, search, track loading |
| 5 | UI | Design tokens, deck widgets, mixer, library browser |
| 6 | Hardware | Controller maps, HID, mapping editor (basic MIDI + trace harness land in Wave 2) |
| 7 | Intelligence | Stem mixing, Automix, sampler |
| 8 | Pro Surface | Recording, broadcast, DVS, video |
| 9 | Release | Legal audit, packaging, docs, v1.0.0 |

## Key Decisions

1. **Clean-room originality** - Features are lawfully copied, expression is not
2. **Depth where DJs spend their time** - waveform, library, mixing and reliability come before breadth; extras do not wait on the engine, and the interface does not wait on extras (ADR 0005)
3. **Precomputed stems** - Offline separation, not real-time
4. **Pure AGPLv3** - No contributor CLA required
5. **Unsigned releases** - Signing is a pre-release decision
6. **Audible vertical slice first** - mixer, MIDI and a minimal debug panel come before time-stretch, FX and heavy dependencies so sound can be tested early
7. **Rust with GPUI** - The application is written in Rust (1.95) with GPUI for the UI: `gpui-pre` 0.3.10, `gpui-kit` 0.8.1 and `gpui-component` (ADR 0004, platform refresh in `openmixer-workbench/docs/spikes/DECISIONS.md`). ADR 0009 (accepted 2026-11-16) moves the interface to a web front end (Tauri v2 with Svelte) in direction; GPUI stays the default and shipping window until the cut-over
8. **Experience is a deliverable** - what a DJ sees is verified by looking at it, with real music where relevant, not by logic tests alone (`docs/PRODUCT.md`, ADR 0005)
9. **Linux first** - Linux is the target for now; Windows and macOS support comes later. CI builds and tests the Rust workspace on Linux only (`ubuntu-24.04`); the macOS and Windows legs return with the porting phase

## Milestones

- **Wave 1 complete**: Rust workspace scaffold, CI matrix and legal framework in place
- **Wave 2 complete**: Audio engine functional with headless soak gate
- **Wave 3 complete**: All analysis pipelines working
- **Wave 4 complete**: Full library functionality
- **Wave 5 complete**: Complete UI with UI-driven 10-minute mix gate
- **Wave 6 complete**: Controller support operational
- **Wave 7 complete**: Intelligence features (stems, Automix)
- **Wave 8 complete**: Pro features
- **Wave 9 complete**: Release-ready v1.0.0

## Current Phases

1. **Engine, analysis, library, MIDI, plugins** - done on Linux, verified headlessly. Open: listening to the real htdemucs model on the ONNX backend (Todo 19; the backend and its toy-model test are built, the export matches Demucs on a reference segment, and the `onnx` CI job is green). The FFmpeg decode fallback (Todo 8b, optional `ffmpeg` feature) covers the sampler, decks, stems and analysis.
2. **UI** - partial: the Machined Faceplate redesign (ADR 0008; plan `openmixer-workbench/docs/superpowers/plans/2026-11-05-ui-faceplate-redesign.md`) is built through Phase 6 and checked on offscreen captures in three themes and both densities, then recaptured from the real window with `tools/screenshots.sh` (X11 on WSLg, 2026-11-12; `docs/screenshots/REVIEW.md`). Open: a live window on a real display, real music, frame times, the owner decisions in `openmixer-workbench/docs/design/faceplate-decisions.md`, and the D7 features (Full/Controller, Compatible with A, vs A, Delta BPM) as their own items. Stem pads (Todo 30) are not checked by ear. The web front end (ADR 0009, accepted 2026-11-16): a Tauri v2 window for the decks screen behind the `gui-tauri` feature, mock device only (`docs/features/tauri-trial.md`); the library, settings and the other panels follow before the cut-over.
3. **Hardware** - MIDI input and mapping done; controller maps and HID (Todos 34-35) open; the mapping editor (Todo 36) is done at logic and view level, not verified with hardware.
4. **Platform refresh** (issues #218 to #229, #241, #250) - done and tested headless: `gpui-kit`/`gpui-component` UI, Rust 1.95, `midir`, `rubato`, `ebur128` loudness metering, media keys and desktop integration, SQLite tuning, bounds-check work. Open: CLAP hosting (#225; first-run prompt, state and latency readout built, delay compensation deferred, nothing third-party run by a person), real-time priority on Linux (#244, on by default, unconfirmed on a device), the EBU 3341/3342 reference files, and the real-hardware pass in `docs/testing/hardware-checklist.md`.
5. **Before Wave 7** - the prioritised list in `openmixer-workbench/TODO.md` (blockers, verification debt, smaller gaps). Done before Wave 7 starts.
6. **Remaining waves** - Wave 7 (stems, Automix, sampler): partial; stem playback, the 8-pad sampler and Automix are in, not heard (mock device only). Wave 8 (recording, broadcast, DVS, video): partial; mix recording is in, not heard; broadcast, DVS and video not started. Wave 9 (release): not started.
7. **Windows and macOS support** - WASAPI and CoreAudio runtime targets, GPUI checks, device runs and the Rubber Band FFI build cost on both. Later. CI is Linux only until then.
8. **Connector for external bots and streaming tools** (issue #261) - unscheduled and opt-in. OpenMixer would not host or ship a bot. It would offer a local connector that other people's bots and tools use: now-playing and performance events, and later the master output as a live stream, with Discord bots as the first target. Nothing is designed or decided here; the issue holds the sketch.
   - Licensing and service terms are the responsibility of the bot's operator or developer; the docs would say so. OpenMixer only provides the connector.

## Current Status

**Status:** Works, verified headlessly on Linux: the audio engine, analysis, the library, MIDI (`midir`, mapping and feedback), loudness metering, in-process and C-ABI plugins (CLAP behind the `clap` feature, in progress), and the GPUI app on Linux. Partial: the UI (Wave 5) and stems (mock separator only). Staged screenshots of the GPUI window have been checked by eye; a live window on a real display has not. A Tauri v2 window for the decks screen (`gui-tauri` feature, mock device only) is built, and ADR 0009 accepted the web front end as the direction for the interface; GPUI stays the default until the cut-over. **Next:** the Before-Wave-7 list in `openmixer-workbench/TODO.md`, then Wave 7.

**Known gaps** are tracked in one issue: [Known gaps: unverified and deferred work](https://github.com/massive-development/openmixer/issues/283).

