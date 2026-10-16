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

## Pull Request Workflow (required)

All changes — including those by AI agents and maintainers — land through a pull request. Never merge a branch directly or push to `main`.

1. Work on a feature branch (`wave2-mixer`, `docs/...`, `fix/...`).
2. Open a PR with `gh pr create`. The description must include:
   - **Summary:** what changed and why (link the Todo number from `TODO.md`).
   - **Changes:** key files/modules added or modified.
   - **Testing:** commands run and results (`ctest`, manual checks).
   - **Follow-ups / known gaps:** anything left undone.
3. Update `TODO.md`, `docs/progress.json` (then run `python3 tools/progress_heatmap.py`) and `docs/PROGRESS.md` in the same PR when task status changes.
4. Merge the PR (`gh pr merge --merge`) once the build and tests pass, so history stays traceable to a PR.

## C++ Standards & Memory Safety (required)

All code must follow modern C++ best practices (C++17 as configured; the C++ Core Guidelines are the reference). Outdated, unsafe or leaking code is not acceptable.

- **Ownership:** RAII everywhere. Use `std::unique_ptr` / `std::make_unique` (or JUCE `OwnedArray`/`Component` ownership); no raw `new`/`delete`, `malloc`/`free`, or owning raw pointers. Use `std::shared_ptr` only when ownership is genuinely shared.
- **Modern idioms:** `auto`, range-for, structured bindings, `std::optional`/`std::variant`/`std::string_view`/`std::span`-style views (or `juce::Span`), `enum class`, `constexpr`, `nullptr`, `override`/`final`, `[[nodiscard]]` where a result must not be ignored. No C-style casts (`static_cast` etc.), no `NULL`, no C arrays where `std::array`/`std::vector` fit.
- **Const-correctness and rule of zero/five:** mark things `const`/`noexcept` where true; follow the rule of zero, otherwise define or delete all special members.
- **Concurrency:** `std::atomic` with explicit memory orders, no data races, no allocation/locks/IO in audio callbacks. Document thread ownership in headers.
- **Bounds & lifetime safety:** no unchecked indexing of untrusted data, no dangling references/iterators, no use of uninitialised memory.
- **Warnings:** code must compile warning-free; do not suppress warnings to pass.
- **Verification before opening a PR:** the test binary must run clean under Valgrind (`valgrind --leak-check=full --error-exitcode=1 <test binary>`) — no leaks, invalid reads/writes or uninitialised-value use — and, where practical, under AddressSanitizer/UBSan/ThreadSanitizer for concurrent code. Report the results in the PR's Testing section.
- **Check current guidance:** when unsure about an API or idiom, consult the C++ Core Guidelines / cppreference / JUCE docs rather than reusing older patterns from memory.

## Comments & Function Size (required)

- **Comments are short.** One line, stating *why* (not what). No paragraph or block comments describing what code does.
- **Long explanation = refactor.** If a function needs a long comment to be understood, split it into smaller, well-named functions instead.
- **Rule of 10 (goal, not a requirement):** aim for functions of about 10 lines or fewer. Exceed it only when splitting would hurt clarity.
- Prefer self-describing names over comments. Delete comments that restate the code.

## Modularity (required)

- **No monolithic files.** One focused responsibility per file/class; split anything that grows large or mixes concerns.
- **Build shared libraries.** Put reusable code in small CMake library targets (e.g. `openmixer_engine`) under `src/<area>/` and link them where needed, instead of duplicating code or piling it into the app target.
- **Keep dependencies one-way** (app → ui → engine/library → core); libraries must not depend on app code and should be testable on their own.

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
