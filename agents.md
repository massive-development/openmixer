# OpenMixer - Agent Instructions

## Overview

OpenMixer is a clean-room, legally original implementation of professional DJ software using JUCE 8. This document provides instructions for any AI agent to understand the codebase and continue development.

## Project Structure

```
/home/brandon/projects/openmixer/
├── .github/workflows/        # CI/CD workflows
├── cmake/                    # CMake configuration
├── docs/
│   ├── adr/                  # Architecture Decision Records
│   └── legal/                # Legal documentation
├── src/
│   └── app/                  # Main application
├── tests/
│   ├── unit/                 # Unit tests
│   └── data/                 # Test fixtures
├── tools/                    # Development tools
└── CMakeLists.txt            # Build system
```

## Current State (Wave 1 Complete)

**Implemented:**
- Repository initialization
- Build system (CMake + JUCE 8)
- CI/CD pipeline (GitHub Actions)
- Governance documents (LICENSE, ADRs, CONTRIBUTING)
- Application skeleton (Main.cpp, MainComponent.cpp)
- License checker tool
- Test harness

**Blocked By:**
- Missing system dependencies (see Build section)

## Architecture Decisions

### ADR-0001: Clean-Room Doctrine
- **Features** are free (methods of operation)
- **Expression** is not (names, logos, artwork)
- No Mixxx code reuse (clean-room)
- All assets must be original

### ADR-0002: Technology Stack
- **Framework:** JUCE 8.0.x
- **License:** AGPLv3
- **Dependencies:** RubberBand, FFmpeg, libsndfile, TagLib, SQLite, ONNX Runtime

### ADR-0003: Licensing
- **Pure AGPLv3** (no Contributor CLA)
- Simple contribution process
- DCO sign-off required

## Build Instructions

### Dependencies

```bash
sudo apt-get install cmake g++ libasound2-dev libx11-dev libxrender-dev \
    libxrandr-dev libxcursor-dev libxi-dev libfreetype-dev \
    libfontconfig1-dev libcurl4-openssl-dev libgtk-3-dev
```

### Build Commands

```bash
# Configure
cmake -B build -DCMAKE_BUILD_TYPE=Release

# Build
cmake --build build

# Run tests
ctest --test-dir build

# Debug with AddressSanitizer
cmake -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build
```

## Implementation Waves

### Wave 1: Foundation ✅ COMPLETE
- Repository setup
- Legal framework
- Build system
- CI/CD
- Test harness

### Wave 2: Audio Engine
- Audio device layer (with CI mock device)
- Deck streaming decoder
- Rubber Band time-stretch
- Mixer (EQ/filter/crossfader)
- FX rack (8 effects)
- Beat sync + quantized cue/loop/slip
- **Gate:** 10-minute headless soak test

### Wave 3: Analysis Pipeline
- Analysis service skeleton
- BPM + beat-grid detection
- Musical key detection
- Waveform data model
- Onset/energy/structure features
- **Stems precompute** (Demucs ONNX)

### Wave 4: Library & Data
- Library schema + DAO layer
- Metadata ingest + watch folders
- Import (iTunes/Rekordbox XML)
- Search + filter engine
- Track prep queue

### Wave 5: Application UI
- Design tokens + LookAndFeel
- Main window layout
- Deck widgets
- Mixer widgets
- Library browser
- Automix panel
- Settings screens
- **Gate:** UI screenshot suite + keyboard-only testing

### Wave 6: Hardware I/O
- MIDI mapping engine
- Controller trace harness
- Plug-and-play maps (4 controllers)
- HID layer
- Mapping editor UI

### Wave 7: Intelligence
- Stem-aware mixing DSP
- Automix transition engine
- 8-pad sampler

### Wave 8: Pro Surface
- Mix recording
- Broadcasting
- DVS support
- Video mixing

### Wave 9: Release Gate
- Legal audit
- Packaging
- v1.0.0 release

## Code Patterns

### Memory Safety

- Use `std::unique_ptr` exclusively
- Never use `new`/`delete` directly
- `JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR` for all classes
- AddressSanitizer in debug builds

### File Structure

```cpp
// Header
#pragma once

#include <JuceHeader.h>

class ClassName : public juce::SomeBase
{
public:
    ClassName();
    ~ClassName() override;

    // Public methods

private:
    // Private members

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (ClassName)
};
```

### Error Handling

- Use `juce::Result` for fallible operations
- Never throw exceptions
- Log errors with `juce::Logger::writeToLog()`

### Testing

- TDD for engine/analysis logic
- Golden-file tests for DSP
- Metric tests for accuracy
- Screenshot tests for UI

## Next Steps (Wave 2)

### Todo 7: Audio Device Layer

**File:** `src/engine/AudioDevice.{h,cpp}`

**Implementation:**
1. JUCE AudioDeviceManager wrapper
2. Platform-specific backends (CoreAudio, WASAPI, ALSA)
3. Mock device for CI testing
4. Latency probing

**Tests:**
```cpp
TEST_CASE("Device opens and closes")
{
    AudioDeviceMock mock;
    mock.open();
    CHECK(mock.isOpen());
    mock.close();
    CHECK(!mock.isOpen());
}
```

### Todo 8: Deck Streaming

**File:** `src/engine/Deck.{h,cpp}`

**Implementation:**
1. Lock-free ring buffer
2. FFmpeg decode backend
3. Transport state machine

**Tests:**
- Golden audio tests
- Transport state transitions

### Todo 9: Time-Stretch

**File:** `src/engine/TimeStretch.{h,cpp}`

**Implementation:**
1. Rubber Band integration
2. Key-lock toggle
3. RT-safety verification

**Tests:**
- Golden audio tests
- Allocation verification

## Common Issues

### Issue: JUCE Font Headers Missing
**Solution:** Install `libfreetype-dev libfontconfig1-dev`

### Issue: X11 Headers Missing
**Solution:** Install `libx11-dev libxrandr-dev libxcursor-dev`

### Issue: JUCE Module Errors
**Solution:** Use pre-fetched JUCE at `build/_tmp/juce-src`

## Verification

After each wave:
1. All tests pass
2. Lint/clang-tidy clean
3. Memory leaks checked (AddressSanitizer)
4. Evidence file updated: `.omo/evidence/task-<N>-openmixer-dj-app.*`

## References

- JUCE 8 API: https://juce.com/learn/api-reference
- Rubber Band: https://breakfastquay.com/rubberband/
- Clean-Room Doctrine: docs/adr/0001-clean-room-doctrine.md
- Full Work Plan: `.omo/plans/openmixer-dj-app.md`
