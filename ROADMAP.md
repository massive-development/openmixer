# OpenMixer Roadmap

This document outlines the plan for OpenMixer. What the product has to be is in `docs/PRODUCT.md`; task-level status is in `TODO.md` and `docs/PROGRESS.md`.

## Tracks

Work runs in parallel tracks, each measured against the product bar. No track waits for another to finish, and Experience is not gated on the engine.

| Track | Outcome a DJ notices | Where it lives today |
|-------|----------------------|----------------------|
| Experience | The screen looks like a product and reads at a glance: spectrum-coloured waveforms with bar numbers and cues, rotary EQ, large labelled pads, a library with artwork and compatibility cues | `TODO.md` "Experience track" |
| Performance | No dropouts over a long set; honest latency; low CPU with four decks and stems | Waves 2 and 9, soak gates |
| Hardware | A controller works when plugged in | Wave 6 |
| Intelligence | Stems, Automix and the sampler save time and sound right | Wave 7 |
| Pro surface | Recording, broadcast, DVS, video | Wave 8 |
| Release | Installable, legal, documented | Wave 9 |

## Waves

The nine implementation waves below are stable identifiers for the tasks in `TODO.md` and `docs/PROGRESS.md`. They are not an order of work.

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
7. **Rust with GPUI** - The application is written in Rust with GPUI for the UI (ADR 0004)
8. **Experience is a deliverable** - what a DJ sees is verified by looking at it, with real music where relevant, not by logic tests alone (`docs/PRODUCT.md`, ADR 0005)
9. **Linux first** - Linux is the target for now; Windows and macOS support comes later. CI still builds and tests the Rust workspace on all three OSes

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

1. **Engine, analysis, library, MIDI, plugins** - done on Linux, verified headlessly. Open: the ONNX stems backend (Todo 19), FFmpeg (Todo 8b, optional).
2. **UI** - partial: decks, mixer and settings are drawn and wired; the design tokens and LookAndFeel (Todo 25) and stem pads (Todo 30) are open; the window has not been checked by eye.
3. **Hardware** - MIDI input and mapping done; controller maps, HID and the mapping editor (Todos 34-36) open.
4. **Before Wave 7** - the prioritised list in `TODO.md` (blockers, verification debt, smaller gaps). Done before Wave 7 starts.
5. **Remaining waves** - Wave 7 (stems, Automix, sampler), Wave 8 (recording, broadcast, DVS, video), Wave 9 (release). Not started.
6. **Windows and macOS support** - WASAPI and CoreAudio runtime targets, GPUI checks, device runs and the Rubber Band FFI build cost on both. Later. CI keeps building and testing on all three OSes throughout.

## Current Status

**Status:** Works, verified headlessly on Linux: the audio engine, analysis, the library, MIDI (ALSA sequencer, mapping and feedback), in-process plugins, and the GPUI app on Linux. Partial: the UI (Wave 5) and stems (mock separator only). The GPUI window has never been viewed by eye. **Next:** the Before-Wave-7 list in `TODO.md`, then Wave 7.

**Known gaps:**

- Automix (Todo 38, Wave 7; the Automix panel waits on it) and the stem features (stem DSP, stem transitions, stem pads: Todos 37, 39, 30)
- The stems ONNX Runtime backend (Todo 19); the separator interface and the licence-gated downloads are done
- HID (Todo 35), the mapping editor UI (Todo 36) and plug-and-play controller maps (Todo 34)
- Dynamic plugin loading: plugins are in-process only; the loading choice is open in `docs/spikes/DECISIONS.md`
- Windows (WASAPI) and macOS (CoreAudio) runtime targets: later. The cpal device has not been run on a real ALSA or PipeWire device
- Visual verification of the GPUI window: the UI verification gate (Todo 31) runs at the logic layer, and nobody has checked the window by eye
