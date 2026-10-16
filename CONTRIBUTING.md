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

## Architecture & Design Patterns (required)

Before starting any large implementation (a new subsystem, a cross-cutting feature, or a new `src/<area>/`):

1. Read `docs/DESIGN_PATTERNS.md` and review how similar code is already structured.
2. Reuse an existing pattern wherever it fits rather than inventing a new one.
3. If a different pattern would improve long-term maintainability (code growth, new features, scaling, testability), pick it deliberately and note the trade-off in the PR.
4. Document every new pattern you introduce in `docs/DESIGN_PATTERNS.md` (use its entry template) in the same PR, so later agents can find and reuse it.

## Token Efficiency & Knowledge Reuse (required)

Consult existing knowledge before computing or exploring. As the codebase grows, keep context small.

1. **Knowledge first:** check, in order, before searching or reading code: `graphify-out/` (query the graph first, e.g. `/graphify query`), any available knowledge base (e.g. the `lumen-kb` MCP `search_kb`), then `docs/PROGRESS.md`, `docs/DESIGN_PATTERNS.md` and `TODO.md`.
2. **Read narrowly:** read only the needed line ranges or symbols, not whole files; prefer grep/glob to locate code. Do not re-read files already in context.
3. **Delegate broad searches** to a search subagent so only conclusions return to the main context.
4. **Keep the graph fresh:** after structural changes (new modules, moved files) re-run graphify locally (`graphify-out/` is git-ignored). Document durable findings in `docs/` rather than rediscovering them.
5. **Don't recompute:** reuse existing results (test output, benchmarks, analysis) instead of re-running or re-deriving them.
6. **Keep outputs lean:** trim command output (`| tail`, `--quiet`), and keep PR descriptions and reports concise.

## Extensibility & User Freedom (required)

Users must always be able to modify OpenMixer with external plugins. Nothing may be locked down.

- **Extension points by default:** new features expose hooks for external plugins — audio pre/post-processing (per deck, channel and master), effects, analysis, library import/export, controller mappings, and UI/theming (colours, layouts, LookAndFeel, skins).
- **No hardcoded restrictions:** don't hardcode behaviour, limits, themes or signal paths that a plugin could reasonably replace or extend. Prefer registries, interfaces and data-driven configuration (JSON/files) over fixed code paths.
- **Stable plugin API:** keep extension interfaces small, documented and versioned; don't break them silently. Plugins must not need to modify core source.
- **No gatekeeping:** don't add signing, allow-lists or sandbox rules that stop users loading their own plugins (opt-in safety warnings are fine). Respect AGPLv3 obligations for plugins linked into the app.
- **PR check:** every PR must state whether it adds, changes or could block an extension point.

## Progress Files (required)

Every PR that completes or advances a task must update, in the same PR:
- `TODO.md` (tick items), `docs/progress.json` (status), `docs/PROGRESS.md` (done/missing detail)
- the heatmap: `python3 tools/progress_heatmap.py` (CI's `progress-check` job fails if `docs/progress-heatmap.svg` is stale)

## CI Workflow Changes (required)

Any change to `.github/workflows/` must be tested locally with [`act`](https://github.com/nektos/act) before opening the PR.

1. Run the affected job(s): `act -j <job> [-W .github/workflows/<file>.yml]` (add `--container-architecture linux/amd64` if needed). List jobs with `act -l`.
2. Fix failures locally; don't push to find out.
3. Put the `act` command and result in the PR's Testing section. Note anything `act` can't cover (e.g. macOS/Windows runners, secrets).

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

See "Comments & Function Size" above. Files get a one-line header (`// OpenMixer <filename> - purpose`); public classes and functions get at most a one-line `///` summary, and only when the name does not already say it.

## Testing

### Test Types

| Type | File Location | Description |
|------|--------------|-------------|
| Unit | `tests/unit/` | Logic verification |
| Golden | `tests/golden/` | DSP correctness |
| Metric | `tests/metrics/` | Accuracy thresholds |
| Screenshot | `tests/screenshots/` | UI verification |

### Adding Tests

Tests use GoogleTest/GoogleMock (fetched by CMake, v1.17.0) in `tests/unit/*Test.cpp`; `TestMain.cpp` initialises JUCE and runs gtest. Add new files to `target_sources(openmixer_tests ...)` in `CMakeLists.txt`.

```cpp
#include <JuceHeader.h>
#include <gtest/gtest.h>

TEST (DeckTest, LoadReportsLength) { /* ASSERT_* for preconditions, EXPECT_* otherwise, EXPECT_NEAR for floats */ }
```

- Use `TEST_F` fixtures for shared setup (unique temp dirs per test so `ctest -j` is safe).
- Suites whose name ends in `Slow` (real threads/timing) get the CTest label `slow`; all others get `unit`. Do not sleep to synchronise: use condition variables or bounded waits.
- Every test is registered with CTest individually (`gtest_discover_tests`).

### Verification Commands

```bash
cmake --preset linux-debug && cmake --build build-linux-debug --target openmixer_tests
ctest --test-dir build-linux-debug --output-on-failure -j4   # everything
ctest --test-dir build-linux-debug -L unit                   # fast subset (-L slow for threaded)
ctest --test-dir build-linux-debug -R Mixer                  # by name regex
ctest --test-dir build-linux-debug -N                        # list tests
build-linux-debug/openmixer_tests_artefacts/Debug/openmixer_tests --gtest_filter='MixerTest.*'

# Sanitizers (ENABLE_ASAN / ENABLE_UBSAN / ENABLE_TSAN; TSan excludes ASan)
cmake --preset linux-asan && cmake --build build-linux-asan --target openmixer_tests && ctest --test-dir build-linux-asan
cmake --preset linux-tsan && cmake --build build-linux-tsan --target openmixer_tests
setarch "$(uname -m)" -R ctest --test-dir build-linux-tsan   # needed on WSL2 kernels (ASLR vs TSan)

# Leaks
valgrind --leak-check=full --error-exitcode=1 build-linux-debug/openmixer_tests_artefacts/Debug/openmixer_tests
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
