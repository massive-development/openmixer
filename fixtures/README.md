# OpenMixer Test Data

Repo-root `fixtures/` holds the small test inputs that Rust tests read.

## Rules
- Only test data, no copyrighted content
- All fixtures must be permissively licensed (CC0, MIT, or similar)
- Keep each file small; generate larger audio in tests instead of checking it in

## Contents
- `controllers/test_map.json`, `controllers/test_trace.json` - controller map and MIDI trace fixtures for `om-midi` tests
- `import/itunes_small.xml`, `import/rekordbox_small.xml` - small hand-written library exports (original content, not copies of any real export), used by the library import tests
