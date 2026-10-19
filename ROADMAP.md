# OpenMixer Roadmap

This document outlines the implementation plan for OpenMixer. The Rust rewrite is governed by the design at `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`; the phased plan for Phases 0-1 is in `docs/superpowers/plans/2026-10-18-rust-phase0-1-scaffold-and-spikes.md`.

## Overview

OpenMixer is being built in 9 implementation waves, organized by feature area:

| Wave | Phase | Focus |
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
2. **Depth-first approach** - Core mixing must be production-ready before extras
3. **Precomputed stems** - Offline separation, not real-time
4. **Pure AGPLv3** - No contributor CLA required
5. **Unsigned releases** - Signing is a pre-release decision
6. **Audible vertical slice first** - mixer, MIDI and a minimal debug panel come before time-stretch, FX and heavy dependencies so sound can be tested early
7. **Rust with GPUI** - The application is rewritten in Rust with GPUI for the UI (ADR 0004, superseding ADR 0002). The frozen C++ baseline (`cpp-final`) was the behavioural reference for parity; the C++ tree was removed once parity was reached
8. **Linux first** - Phase 2 and later target Linux only; Windows and macOS are a later porting phase. CI still builds and tests the Rust workspace on all three OSes

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

## Rust Rewrite Phases

0. **Scaffolding** - workspace, CI matrix, lint and deny configuration, ADR 0004, docs rewrite, parity inventory. Done.
1. **Spikes** (throwaway, labelled) - GPUI window with a custom waveform view on Linux (the go/no-go gate, closed on Linux); cpal output callback; time-stretch quality (decided: Rubber Band R3 via FFI behind a `Stretcher` trait). Done on Linux.
2. **Engine** - audio I/O, engine, headless mix test and soak gate, plus the `om-stretch` crate. Done.
3. **Analysis and library** - analysis against golden vectors from the baseline, then the library. Done, except the ONNX stems backend (Todo 19).
4. **MIDI** - mapper, learn, feedback. Done.
5. **UI** - UI over the stable API; the UI-driven gate at the logic layer. Done, with partial decks, mixer and settings, and the design tokens (Todo 25) and stem pads (Todo 30) open. The C++ tree was removed once parity was reached.
6. **Remaining waves** - stems, Automix, pro surface, release. Waves 7-9 were never in the C++ baseline. Not started.
7. **Porting** - Windows (WASAPI) and macOS (CoreAudio) runtime targets: GPUI checks, device runs, listening tests and the Rubber Band FFI build cost on both. Next. CI keeps building and testing on all three OSes throughout.

## Current Status

**Status:** Rust port at parity with the C++ baseline (Waves 1-6); Linux first; Windows/macOS porting phase and Waves 7-9 next.

The C++ implementation was removed in this commit range. It is preserved at the `cpp-final` tag (commit c6d2747), and the MIDI learn and feedback C++ is on `origin/wave6-midi-learn-feedback`.

**Known gaps:**

- Automix (Todo 38, Wave 7; the Automix panel waits on it) and the stem features (stem DSP, stem transitions, stem pads: Todos 37, 39, 30)
- The stems ONNX Runtime backend (Todo 19); the separator interface and the licence-gated downloads are done
- HID (Todo 35), the mapping editor UI (Todo 36) and plug-and-play controller maps (Todo 34)
- Dynamic plugin loading: plugins are in-process only; the loading choice is open in `docs/spikes/DECISIONS.md`
- Windows (WASAPI) and macOS (CoreAudio) runtime targets: the porting phase. The cpal device has not been run on a real ALSA or PipeWire device
- Visual verification of the GPUI window: the UI verification gate (Todo 31) runs at the logic layer, and nobody has checked the window by eye
