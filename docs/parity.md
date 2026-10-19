# C++ to Rust parity inventory

This table is generated from the `cpp-final` git tag (the frozen C++ baseline) by `tools/parity_inventory.py`; do not edit it by hand. To regenerate, run `python3 -I tools/parity_inventory.py` from the repo root. Each row is one `**Todo N:**` entry in that tag's `TODO.md`. The C++ column lists the test and tool files that the Todo names, plus the `tests/unit` and `tools` sources the Rust tests say they port. The Rust column lists the crate test files (and inline test modules) that replace them. Status comes from `docs/progress.json`: `done`, `partial` or `todo`. Todos that the C++ baseline never implemented (Waves 7-9, and Todos 30, 34-36) say so in the C++ column.

| Todo | Title | C++ test(s) | Rust crate(s) / test file(s) | Status |
| --- | --- | --- | --- | --- |
| 7 | Audio device layer + latency instrumentation + mock device | `AudioDeviceTest.cpp` | `crates/om-audio/tests/audio_device.rs`, `crates/om-audio/tests/cpal_device.rs` | done |
| 8 | Deck engine (streaming decode + transport) | `DeckTest.cpp`, `DjEngineTest.cpp` | `crates/om-engine/src/deck/tests.rs`, `crates/om-engine/src/dj_engine/tests.rs`, `crates/om-audio/tests/dj_engine.rs` | done |
| 10 | Mixer (channels, EQ, filter, crossfader) | `MixerTest.cpp` | `crates/om-engine/tests/mixer.rs` | done |
| 10a | Plugin API + registry (extension points) | `PluginRegistryTest.cpp` | `crates/om-plugins/tests/plugin_registry.rs`, `examples/plugins/gain-plugin/tests/gain_plugin.rs`, `crates/openmixer/src/plugins.rs` | done |
| 32 | MIDI input + mapping engine (pulled forward from Wave 6) | `MidiMapperTest.cpp` | `crates/om-midi/tests/midi_device.rs`, `crates/om-midi/tests/midi_mapper.rs`, `crates/om-midi/tests/midi_learn.rs`, `crates/om-midi/tests/alsa_backend.rs` | done |
| 33 | Controller trace harness (pulled forward from Wave 6) | `MidiMapperTest.cpp` | `crates/om-midi/tests/midi_mapper.rs`, `fixtures/controllers` | done |
| 26a | Minimal debug panel (pulled forward from Wave 5) | — | `crates/om-ui-gpui/src/debug_panel.rs` | partial |
| 13a | Short soak test in CI (30-60s headless mix) | `soak_mix.cpp` | `crates/om-audio/tests/soak.rs` | done |
| 9 | Key-lock time-stretch via Rubber Band | `DeckKeylockTest.cpp` | `crates/om-engine/src/deck/keylock_tests.rs`, `crates/om-stretch/src/rubberband.rs` | done |
| 12 | Beat-sync + quantized cue/loop/slip (partial: grid interface, sync, quantizer, slip, gapless Deck loop playback + hotcues done; ids wired through `DeckControl`: `deck.N.sync`, `sync.master`, `loop.quantized.<beats>`, `slip`; real grids Wave 3, UI open) | `DeckLoopTest.cpp`, `EngineWiringTest.cpp` | `crates/om-engine/src/sync.rs`, `crates/om-engine/src/quantizer.rs`, `crates/om-engine/src/slip.rs`, `crates/om-engine/src/deck/loop_tests.rs`, `crates/om-audio/tests/engine_wiring.rs` | done |
| 11 | FX rack (8 essential effects) | `FxTest.cpp` | `crates/om-engine/tests/fx.rs`, `crates/om-engine/src/fx/rack.rs`, `crates/om-engine/src/fx_routing.rs` | done |
| 8b | FFmpeg decode backend (optional, after the above) | — | — | todo |
| 13 | Engine soak test (10-minute headless mix) | `soak_mix.cpp` | `crates/om-audio/tests/soak.rs` | done |
| 14 | Analysis service skeleton + SQLite cache | `AnalysisCacheTest.cpp`, `AnalysisServiceTest.cpp` | `crates/om-analysis/src/service/tests.rs`, `crates/om-analysis/src/sqlite_cache/tests.rs`, `crates/om-analysis/src/registry/tests.rs`, `crates/om-analysis/src/clip_source/tests.rs` | done |
| 15 | BPM + beat-grid detection | `BpmEstimatorTest.cpp` | `crates/om-analysis/src/bpm/tests.rs` | done |
| 16 | Musical key detection + Camelot mapping | `KeyEstimatorTest.cpp` | `crates/om-analysis/src/key/tests.rs` | done |
| 17 | Waveform data model | `WaveformBuilderTest.cpp` | `crates/om-analysis/src/waveform/tests.rs` | done |
| 18 | Onset/energy/structure features | `StructureAnalyzerTest.cpp` | `crates/om-analysis/src/structure/tests.rs` | done |
| 19 | Stems precompute (Demucs ONNX) (partial: separator interface, model manifest, licence-gated verified download done; ONNX Runtime backend open) | `StemsTest.cpp` | `crates/om-stems/tests/stems.rs` | partial |
| 20 | Library schema + DAO layer | `LibraryDaoTest.cpp`, `LibraryDatabaseTest.cpp` | `crates/om-library/tests/library_dao.rs`, `crates/om-library/tests/library_database.rs` | done |
| 21 | Metadata ingest + watch folders (partial: RIFF INFO tags, duration, watch folders done; TagLib tags for MP3/FLAC/M4A and artwork open) | `LibraryIngestTest.cpp`, `LibraryWatchFoldersTest.cpp` | `crates/om-library/tests/library_ingest.rs`, `crates/om-library/tests/metadata_reader.rs`, `crates/om-library/tests/library_watch_folders.rs` | done |
| 22 | Import (iTunes/Rekordbox XML) | `LibraryImportTest.cpp` | `crates/om-library/tests/library_import.rs` | done |
| 23 | Search + filter engine | `LibrarySearchTest.cpp` | `crates/om-library/tests/library_search.rs`, `crates/om-library/tests/library_search_properties.rs` | done |
| 24 | Track prep queue | `LibraryPrepQueueTest.cpp` | `crates/om-library/tests/library_prep_queue.rs` | done |
| 25a | Theme/skin loader (done: JSON themes, `ThemeRegistry` with built-in/user/plugin sources and live switching, `PluginHost::registerTheme`; plugin theme forwarding, theme picker UI + persisted choice; missing: layouts, user-theme-dir file watching) | `ThemeJsonTest.cpp`, `ThemeRegistryTest.cpp` | `crates/om-ui/tests/theme_json.rs`, `crates/om-ui/tests/theme_registry.rs` | done |
| 25 | Design tokens + LookAndFeel | — | `crates/om-ui/src/theme/tokens.rs` | todo |
| 26 | Main window + deck widgets (done: window, widgets and tests; missing: a real main-cue point; the library feed landed with Todo 28) | `DeckWidgetTest.cpp`, `MainLayoutTest.cpp` | `crates/om-ui/tests/deck_widgets.rs`, `crates/om-ui/tests/main_layout.rs` | partial |
| 27 | Mixer widgets + metering (done: strips, EQ + kill, crossfader + curve, master, cue, FX panel, stereo peak meters with hold + clip; missing: RMS meters, FX assign beyond the per-channel rack shortcut) | `FxPanelTest.cpp`, `MixerWidgetTest.cpp` | `crates/om-ui/tests/mixer_widgets.rs` | partial |
| 28 | Library browser + app wiring (done: sidebar tree, paged sortable table with Camelot compatibility tint, search box on the query syntax, load buttons/double-click/drag to decks, prep queue view + enqueue, iTunes/Rekordbox import, `LibraryServices` composition (library, analysis cache, prep queue pumped on a timer, watch folders), `TrackLoader` feeds waveform + beat grid + metadata to decks; missing: playlist editing, prep queue reorder/remove UI, auto-enqueue after scans) | `LibraryBrowserTest.cpp`, `TrackLoaderTest.cpp` | `crates/om-ui/tests/library_browser.rs`, `crates/om-ui/tests/track_loader.rs`, `crates/om-ui-gpui/tests/library_panel.rs` | done |
| 29 | Automix panel + settings (done: settings overlay with audio device/rate/buffer/master+headphone output pairs + latency/xrun/CPU, MIDI inputs + map file, library watch folders + rescan, theme picker, plugin folder; versioned `preferences.json` applied at start and on change; plugins loaded at start with their themes forwarded; missing: Automix panel (Wave 7, needs Todo 38), live scan-interval change) | — | `crates/om-ui/tests/library_settings.rs`, `crates/om-ui/src/settings.rs`, `crates/om-ui/src/preferences.rs`, `crates/om-ui/src/preferences_applier.rs`, `crates/openmixer/tests/audio_settings.rs`, `crates/om-ui-gpui/src/settings_view.rs` | partial |
| 30 | Stem pads UI (open: waits on Wave 7 stem playback) | not in C++ baseline | — | todo |
| 31 | UI verification gate | `ui_gate.cpp` | `crates/openmixer/tests/ui_gate.rs`, `crates/openmixer/tests/smoke.rs` | done |
| 34 | Plug-and-play controller maps | not in C++ baseline | — | todo |
| 35 | HID layer | not in C++ baseline | — | todo |
| 36 | Mapping editor UI | not in C++ baseline | — | todo |
| 37 | Stem-aware mixing DSP | not in C++ baseline | — | todo |
| 38 | Automix transition engine | not in C++ baseline | — | todo |
| 39 | Stem transitions | not in C++ baseline | — | todo |
| 40 | 8-pad sampler | not in C++ baseline | — | todo |
| 41 | Mix recording | not in C++ baseline | — | todo |
| 42 | Broadcasting | not in C++ baseline | — | todo |
| 43 | DVS (timecode vinyl) | not in C++ baseline | — | todo |
| 44 | Video mixing | not in C++ baseline | — | todo |
| 47 | Legal audit | not in C++ baseline | — | todo |
| 48 | Trademark check | not in C++ baseline | — | todo |
| 49 | Packaging | not in C++ baseline | — | todo |
| 50 | Documentation | not in C++ baseline | — | todo |
| 51 | v1.0.0 release | not in C++ baseline | — | todo |
