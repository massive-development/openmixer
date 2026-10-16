# Contributing to OpenMixer

Thank you for your interest in contributing to OpenMixer! This document outlines the conventions for both AI and human contributors.

## Quick Start

### For AI Agents

1. **Read `agents.md` first** - Contains complete setup instructions
2. **Check the work plan** - `.omo/plans/openmixer-dj-app.md`
3. **Create/update tasks** - Use `todowrite` tool to track progress
4. **Verify each change** - Run `lsp_diagnostics` and tests

### For Humans

1. Set up development environment (see `agents.md`)
2. Fork the repository
3. Create a feature branch
4. Follow the clean-room doctrine

## Development Workflow

### AI Agent Checklist

**Before implementing:**
- [ ] Read `agents.md` for context
- [ ] Identify the wave/todo number
- [ ] Check ADRs for design decisions
- [ ] Verify no existing implementation exists

**While implementing:**
- [ ] Follow existing code patterns
- [ ] Add/expand tests
- [ ] Run AddressSanitizer in debug builds
- [ ] Create evidence file: `.omo/evidence/task-<N>-openmixer-dj-app.*`

**After completing:**
- [ ] All tests pass
- [ ] `lsp_diagnostics` clean
- [ ] Update ADR if design changes
- [ ] Document in evidence file

### Human Contributor Guidelines

**Git Workflow:**
```bash
# Create feature branch
git checkout -b feature/feature-name

# Make changes with DCO sign-off
git commit -s -m "feat: add feature description"

# Push and create PR
git push origin feature/feature-name
```

**DCO Sign-off:**
All commits must include a `Signed-off-by` line to certify you have the right to submit the code under AGPLv3.

```bash
git commit -s -m "fix: description"
```

## Code Style

### Naming Conventions

- **Classes:** `PascalCase`
- **Methods:** `camelCase`
- **Constants:** `SCREAMING_SNAKE_CASE`
- **Files:** `snake_case`

### File Organization

```
src/
├── app/              # Application entry points
├── engine/           # Core audio engine
├── analysis/         # Analysis pipeline
├── library/          # Library management
├── ui/               # User interface
├── controllers/      # Hardware I/O
└── stems/            # Stem separation
```

### Comment Requirements

**Files:**
```cpp
/*
  OpenMixer <filename>
  Brief description of file's purpose
*/
```

**Classes:**
```cpp
/**
  ClassName brief description

  Detailed description if needed.
  @see RelatedClass
*/
```

**Functions:**
```cpp
/**
  Brief description of function

  @param param Description
  @return Description
*/
```

## Testing

### Test Types

| Type | File Location | Description |
|------|--------------|-------------|
| Unit | `tests/unit/` | Logic verification |
| Golden | `tests/golden/` | DSP correctness |
| Metric | `tests/metrics/` | Accuracy thresholds |
| Screenshot | `tests/screenshots/` | UI verification |

### Adding Tests

```cpp
// tests/unit/AudioEngineTest.cpp
#include <JuceHeader.h>

TEST_CASE("Deck can load audio")
{
    AudioFileReader reader;
    auto buffer = reader.loadFile("fixture.wav");
    CHECK(buffer.getNumSamples() > 0);
}
```

### Verification Commands

```bash
# Run all tests
ctest --output-on-failure

# Run specific test
ctest -R audio_engine

# With AddressSanitizer
ctest --test-dir build
```

## Build Requirements

### Minimum System

- **CMake** 3.22+
- **GCC/Clang** with C++17 support
- **Audio Libraries** (see agents.md)

### Debug Build

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build

# Memory leak detection
valgrind --leak-check=full ./build/openmixer
```

## Documentation

### Architecture Decision Records (ADRs)

When making architectural decisions:

1. Create `docs/adr/NNNNN-title.md`
2. Follow ADR template:
   - Status (Proposed/Accepted/Rejected)
   - Context
   - Decision
   - Consequences
3. Update `docs/adr/INDEX.md`

### Evidence Files

After each major task:

```
.omo/evidence/task-<N>-openmixer-dj-app.md

## Task N: Description

**Status:** Complete

**Files changed:**
- src/engine/...

**Tests:**
- All pass

**Evidence:**
- Test results
- Screenshot (if UI)
- Performance metrics
```

## Clean-Room Compliance

**Never:**
- Copy djay/Algoriddim names/logos/artwork
- Copy Mixxx application code
- Bundle ML model weights
- Integrate Spotify

**Always:**
- Use generic names ("Deck", not "Deck 1")
- Implement features independently
- Download weights on first run
- Document asset provenance

## PR Requirements

All pull requests must:

1. Pass all CI checks
2. Include tests
3. Have evidence file (for AI work)
4. Update ADR if applicable
5. Follow code style

## Support

**Documentation:**
- `agents.md` - Full setup and implementation guide
- `README.md` - Project overview
- `docs/adr/` - Architecture decisions
- `docs/legal/` - Legal documentation

**Work Plan:**
- `.omo/plans/openmixer-dj-app.md` - Complete implementation roadmap
- `.omo/drafts/` - Draft specifications

---

*Thank you for contributing to OpenMixer!*
