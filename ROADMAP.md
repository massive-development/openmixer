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
7. **Rust with GPUI** - The application is rewritten in Rust with GPUI for the UI (ADR 0004, superseding ADR 0002). The frozen C++ baseline (`cpp-final`) is the behavioural reference until Phase 5 passes

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

0. **Scaffolding** - workspace, CI matrix, lint and deny configuration, ADR 0004, docs rewrite, parity inventory. Scaffold, CI and ADR 0004 have landed; docs rewrite is in progress.
1. **Spikes** (throwaway, labelled) - GPUI window with a custom waveform view on Linux, Windows and macOS (the go/no-go gate); cpal output callback; time-stretch quality. Next.
2. **Engine** - audio I/O, engine, headless mix test and soak gate. Planned.
3. **Analysis and library** - analysis against golden vectors from the baseline, then the library. Planned.
4. **MIDI** - mapper, learn, feedback. Planned.
5. **UI** - UI over the stable API; UI-driven 10-minute mix gate. Planned. The C++ tree is deleted once this passes.
6. **Remaining waves** - stems, Automix, pro surface, release. Planned.

## Current Status

**Phase**: Phase 0 in progress (plan tasks 1-4 done, task 5 docs rewrite in review, 6-9 pending); Phase 1 (spikes) next. Wave 1 is complete for the Rust scaffold; the other waves are not started in Rust.
