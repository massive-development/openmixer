# OpenMixer Test Data

Repo-root `fixtures/` holds the small test inputs that Rust tests read.

## Rules
- Only test data, no copyrighted content
- All fixtures must be permissively licensed (CC0, MIT, or similar)
- Keep each file small; generate larger audio in tests instead of checking it in

## Contents
- `controllers/test_map.json`, `controllers/test_trace.json` - controller map and MIDI trace fixtures for `om-midi` tests
- `import/itunes_small.xml`, `import/rekordbox_small.xml` - small hand-written library exports (original content, not copies of any real export), used by the library import tests
- `controllers/generic_mixer_trace.json`, `controllers/generic_pads_trace.json` - MIDI traces replayed against the generic maps in `resources/controllers/` (`om-midi/tests/controller_maps.rs`)
- `controllers/record_map.json`, `controllers/sampler_map.json`, `controllers/stem_map.json` - example maps for the recorder, sampler and stem targets
- `crates/om-stems/tests/fixtures/htdemucs_reference.json` (outside this folder) - what Demucs produces for a deterministic segment, written by `tools/htdemucs_reference.py`

Controller captures for named devices belong here too, with a provenance note in `docs/controllers/` (ADR 0006).
