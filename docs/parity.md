# C++ to Rust parity inventory

This table is generated from the `cpp-final` git tag (the frozen C++ baseline) by `tools/parity_inventory.py`; do not edit it by hand. To regenerate, run `python3 -I tools/parity_inventory.py` from the repo root. Each row is one `**Todo N:**` entry in that tag's `TODO.md`. The C++ test column lists test and tool files under `tests/` and `tools/` that the Todo names explicitly. The Rust columns are placeholders until the Rust side is filled in.

| Todo | Title | C++ test(s) | Rust test | Status |
| --- | --- | --- | --- | --- |
| 7 | Audio device layer + latency instrumentation + mock device | — | — | todo |
| 8 | Deck engine (streaming decode + transport) | — | — | todo |
| 10 | Mixer (channels, EQ, filter, crossfader) | — | — | todo |
| 10a | Plugin API + registry (extension points) | — | — | todo |
| 32 | MIDI input + mapping engine (pulled forward from Wave 6) | — | — | todo |
| 33 | Controller trace harness (pulled forward from Wave 6) | — | — | todo |
| 26a | Minimal debug panel (pulled forward from Wave 5) | — | — | todo |
| 13a | Short soak test in CI (30-60s headless mix) | `soak_mix.cpp` | — | todo |
| 9 | Key-lock time-stretch via Rubber Band | — | — | todo |
| 12 | Beat-sync + quantized cue/loop/slip (partial: grid interface, sync, quantizer, slip, gapless Deck loop playback + hotcues done; ids wired through `DeckControl`: `deck.N.sync`, `sync.master`, `loop.quantized.<beats>`, `slip`; real grids Wave 3, UI open) | — | — | todo |
| 11 | FX rack (8 essential effects) | — | — | todo |
| 8b | FFmpeg decode backend (optional, after the above) | — | — | todo |
| 13 | Engine soak test (10-minute headless mix) | `soak_mix.cpp` | — | todo |
| 14 | Analysis service skeleton + SQLite cache | — | — | todo |
| 15 | BPM + beat-grid detection | — | — | todo |
| 16 | Musical key detection + Camelot mapping | — | — | todo |
| 17 | Waveform data model | — | — | todo |
| 18 | Onset/energy/structure features | — | — | todo |
| 19 | Stems precompute (Demucs ONNX) (partial: separator interface, model manifest, licence-gated verified download done; ONNX Runtime backend open) | — | — | todo |
| 20 | Library schema + DAO layer | — | — | todo |
| 21 | Metadata ingest + watch folders (partial: RIFF INFO tags, duration, watch folders done; TagLib tags for MP3/FLAC/M4A and artwork open) | — | — | todo |
| 22 | Import (iTunes/Rekordbox XML) | — | — | todo |
| 23 | Search + filter engine | — | — | todo |
| 24 | Track prep queue | — | — | todo |
| 25a | Theme/skin loader (done: JSON themes, `ThemeRegistry` with built-in/user/plugin sources and live switching, `PluginHost::registerTheme`; plugin theme forwarding, theme picker UI + persisted choice; missing: layouts, user-theme-dir file watching) | — | — | todo |
| 25 | Design tokens + LookAndFeel | — | — | todo |
| 26 | Main window + deck widgets (done: window, widgets and tests; missing: a real main-cue point; the library feed landed with Todo 28) | — | — | todo |
| 27 | Mixer widgets + metering (done: strips, EQ + kill, crossfader + curve, master, cue, FX panel, stereo peak meters with hold + clip; missing: RMS meters, FX assign beyond the per-channel rack shortcut) | — | — | todo |
| 28 | Library browser + app wiring (done: sidebar tree, paged sortable table with Camelot compatibility tint, search box on the query syntax, load buttons/double-click/drag to decks, prep queue view + enqueue, iTunes/Rekordbox import, `LibraryServices` composition (library, analysis cache, prep queue pumped on a timer, watch folders), `TrackLoader` feeds waveform + beat grid + metadata to decks; missing: playlist editing, prep queue reorder/remove UI, auto-enqueue after scans) | — | — | todo |
| 29 | Automix panel + settings (done: settings overlay with audio device/rate/buffer/master+headphone output pairs + latency/xrun/CPU, MIDI inputs + map file, library watch folders + rescan, theme picker, plugin folder; versioned `preferences.json` applied at start and on change; plugins loaded at start with their themes forwarded; missing: Automix panel (Wave 7, needs Todo 38), live scan-interval change) | — | — | todo |
| 30 | Stem pads UI (open: waits on Wave 7 stem playback) | — | — | todo |
| 31 | UI verification gate | `ui_gate.cpp` | — | todo |
| 34 | Plug-and-play controller maps | — | — | todo |
| 35 | HID layer | — | — | todo |
| 36 | Mapping editor UI | — | — | todo |
| 37 | Stem-aware mixing DSP | — | — | todo |
| 38 | Automix transition engine | — | — | todo |
| 39 | Stem transitions | — | — | todo |
| 40 | 8-pad sampler | — | — | todo |
| 41 | Mix recording | — | — | todo |
| 42 | Broadcasting | — | — | todo |
| 43 | DVS (timecode vinyl) | — | — | todo |
| 44 | Video mixing | — | — | todo |
| 47 | Legal audit | — | — | todo |
| 48 | Trademark check | — | — | todo |
| 49 | Packaging | — | — | todo |
| 50 | Documentation | — | — | todo |
| 51 | v1.0.0 release | — | — | todo |
