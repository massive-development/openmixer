# OpenMixer Roadmap

This document outlines the implementation plan for OpenMixer. For technical details, see the full work plan at `.omo/plans/openmixer-dj-app.md`.

## Overview

OpenMixer is being built in 9 implementation waves, organized by feature area:

| Wave | Phase | Focus |
|------|-------|-------|
| 1 | Foundation | Repo setup, legal rulebook, build system, CI, test harness |
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

## Milestones

- **Wave 1 complete**: Project bootstrapped, legal framework in place
- **Wave 2 complete**: Audio engine functional with performance gate
- **Wave 3 complete**: All analysis pipelines working
- **Wave 4 complete**: Full library functionality
- **Wave 5 complete**: Complete UI with 10-minute mix test gate
- **Wave 6 complete**: Controller support operational
- **Wave 7 complete**: Intelligence features (stems, Automix)
- **Wave 8 complete**: Pro features
- **Wave 9 complete**: Release-ready v1.0.0

## Current Status

**Phase**: Wave 1 - Foundation

Progressing through initial repository setup, governance documentation, and build system establishment.
