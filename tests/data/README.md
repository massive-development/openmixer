# OpenMixer Test Data
This directory contains test fixtures and reference data.

## Rules
- Only test data, no copyrighted content
- All fixtures must be permissively licensed (CC0, MIT, or similar)
- Large audio files should be downloaded via tools/fetch_test_data.py

## Test Fixtures
- `reference/` - Reference audio files for golden tests
- `fixtures/` - Small test files (< 1MB each)

## Test Data Fetcher
Run `tools/fetch_test_data.py` to download required test fixtures:
```bash
python3 tools/fetch_test_data.py
```
