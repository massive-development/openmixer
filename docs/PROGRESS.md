# OpenMixer Progress & Parity Tracker

Where the project stands against the goal: a clean-room, djay-Pro-class DJ application.
`TODO.md` is the task list; this page explains **what each task actually entails, what exists today, and what is still missing** — use it to find where to contribute.

Legend: ✅ done · 🟡 partial / in progress · ⬜ not started

_Last updated: after Wave 2 Todos 7–8. Update this file in the same PR that changes a task's status._

## At a glance

![OpenMixer progress heatmap](docs/progress-heatmap.svg)

_Regenerate with `python3 tools/progress_heatmap.py` after editing `docs/progress.json`. Weighting: done = 1, partial = 0.5, in progress = 0.25._

| Wave | Area | Status |
|------|------|--------|
| 1 | Foundation | ✅ |
| 2 | Audio engine | 🟡 device, deck, mixer, MIDI mapper + trace harness, debug panel done; time-stretch, FX, sync, soak remain |
| 3 | Analysis | ⬜ |
| 4 | Library | ⬜ |
| 5 | UI | ⬜ (window shell only) |
| 6 | Hardware | ⬜ (basic MIDI pulled into Wave 2) |
| 7 | Intelligence | ⬜ |
| 8 | Pro surface | ⬜ |
| 9 | Release | ⬜ |

**What you can do today:** build and run the app: load files into two decks, play, mix with EQ/filter/crossfader/PFL, and drive it from a MIDI controller via a JSON map; run the unit tests.

---

## Wave 1 — Foundation ✅

| Task | Entails | Status |
|------|---------|--------|
| Repo, governance, legal | AGPLv3, clean-room ADRs, contributing/CoC, asset provenance CSV, `tools/license_check.py` | ✅ |
| Build system | CMake + JUCE 8.0.4 via FetchContent, Linux/macOS/Windows settings, presets | ✅ (Linux verified; macOS/Windows untested) |
| CI | `.github/workflows/ci.yml` | 🟡 present; not yet confirmed green on all platforms |
| Test harness | `juce::UnitTest` runner, `ctest` integration, `--smoke` app mode | ✅ |

**Missing:** verified macOS and Windows builds; packaging.

---

## Wave 2 — Audio engine 🟡

Goal: reliable, low-latency playback of two decks through a mixer, controllable by MIDI, testable headlessly.

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 7 Audio device layer | Device abstraction, latency/xrun instrumentation, headless mock for CI | ✅ `AudioDevice`, `LatencyMonitor`, `JuceAudioDevice`, `MockAudioDevice` | Real-hardware test of `JuceAudioDevice`; device selection/settings UI |
| 8 Deck engine | Streaming decode, lock-free buffer, play/pause/seek/rate | ✅ `Deck`, `RingBuffer`; JUCE decoders (WAV/AIFF/FLAC/OGG, MP3 where JUCE supports it) | FFmpeg backend (8b) for wide codec support; scratching |
| 10 Mixer | Channel gain, 3-band EQ, filter, crossfader (curves), PFL/cue bus, master + headphone out | ✅ | Headphone routing to a second physical device; EQ/filter smoothing |
| 10a Plugin API | Versioned plugin interfaces, runtime loader, registry; hooks for audio pre/post-processing, FX, MIDI maps | ✅ `src/plugins/` (`openmixer_plugins`): `PluginApi`, `PluginLoader`, `PluginRegistry`, `StageCatalog`; example plugin | Per-deck insert hook (decks use channel inserts); app/UI wiring and plugin-folder settings; plugin-provided analysis/importers/themes (later waves) |
| 32 MIDI input + mapping | `MidiInput` → JSON map → engine parameters | ✅ | Learn mode; feedback/LED output (Wave 6) |
| 33 Controller trace harness | Replay recorded MIDI streams headlessly | ✅ | Real controller recordings |
| 26a Debug panel | Two decks, faders, EQ, meters on plain JUCE widgets | ✅ (not visually verified: no display in CI box) | Replaced by real UI in Wave 5 |
| 13a Short soak test | 30–60 s headless mix in CI | ✅ `tools/soak_mix.cpp` (`openmixer_soak`, CTest `soak_short`, label `soak`): 2 looping decks, moving controls, asserts 0 xruns, 0 steady-state underruns, no NaN/Inf/overflow | Runs on a paced mock device, not real hardware; not yet in CI (workflow owned elsewhere) |
| 9 Key-lock time-stretch | Rubber Band realtime, tempo change without pitch change | ⬜ | Dependency integration (GPL-compatible), golden audio tests |
| 12 Sync & quantize | Beat-phase alignment, quantized cue/loop, slip mode | 🟡 `BeatGrid` interface + `FixedTempoGrid`, `Sync` (tempo match + beat/bar phase nudge, `ParameterTarget`), `Quantizer` (snap, quantized loop bounds), `SlipTracker`; tested with mock grids; `Deck` gapless loops (sample-accurate wrap into a decoder-prefetched `LoopPrefetch` slot, phase-kept resize, exit/reloop) and 8 hotcues (`HotcueBank`, ids `hotcueN`, `hotcueN.clear`, `loop.exit`, `loop.reloop`) | Real grids (Wave 3); wiring `Sync::apply`, `Quantizer` loops and `SlipTracker` into the app/UI and MIDI maps; hotcue persistence (Wave 4 library) |
| 11 FX rack | 8 effects (delay, reverb, flanger, …) with wet/dry and beat-sync | ⬜ | All effects; original presets only (no copied FX presets) |
| 13 Full soak test | 10-minute headless mix, glitch/xrun budget | ⬜ | The Wave 2 gate |

**Parity gap vs. pro DJ apps in this wave:** key-lock, sync, loops/hotcues, FX, 4-deck support, recording-quality resampling.

---

## Wave 3 — Analysis ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 14 Analysis service + cache | Background job runner, SQLite result cache keyed by file hash | All |
| 15 BPM + beat grid | Tempo estimation, grid with downbeat, manual correction data model | All |
| 16 Key detection | Chroma-based key, Camelot mapping | All |
| 17 Waveform model | Multi-resolution overview/zoom data, colour/band data | All |
| 18 Structure features | Onset/energy/section detection (feeds Automix) | All |
| 19 Stem precompute | Demucs ONNX, weights downloaded on first use with license shown | All |

## Wave 4 — Library ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 20 Schema + DAO | Tracks, playlists, crates, history | All |
| 21 Metadata ingest | Tag reading, artwork, watch folders | All |
| 22 Import | iTunes / Rekordbox XML | All |
| 23 Search/filter | Text, BPM/key range, smart filters | All |
| 24 Prep queue | Analyse-on-import pipeline | All |

## Wave 5 — UI ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 25 Design tokens + LookAndFeel | Original visual identity (no copied layouts/artwork) | All |
| 25a Theme/skin loader | User themes, colours and layouts loaded via the plugin API | All |
| 26 Main window + decks | Waveforms, transport, hotcues, loops | All (a placeholder window exists) |
| 27 Mixer widgets | Knobs, faders, meters | All |
| 28 Library browser | Tree, track list, search | All |
| 29 Automix panel + settings | Audio/MIDI settings, preferences | All |
| 30 Stem pads UI | Per-stem mute/volume | All |
| 31 UI verification gate | 10-minute mix test through the UI | All |

## Wave 6 — Hardware ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 34 Controller maps | Plug-and-play maps for popular controllers | All |
| 35 HID layer | Non-MIDI controllers | All |
| 36 Mapping editor UI | In-app learn/edit | All |

## Waves 7–9 ⬜

| Wave | Contents | Status |
|------|----------|--------|
| 7 | Stem-aware mixing, Automix, stem transitions, 8-pad sampler | ⬜ |
| 8 | Mix recording, broadcasting, DVS (timecode vinyl), video mixing | ⬜ |
| 9 | Legal audit, trademark check, packaging, docs, v1.0.0 | ⬜ |

---

## Known out-of-scope / non-goals

- Spotify or other streaming-service integration (terms of service prohibit DJ mixing).
- Mobile and web apps.
- Real-time stem separation (stems are precomputed offline).

## How to help

1. Pick a ⬜ or 🟡 row, open an issue naming the Todo number.
2. Read `CONTRIBUTING.md` and `docs/adr/0001-clean-room-doctrine.md` before writing code.
3. Include tests; engine code must not allocate or lock in audio callbacks.
