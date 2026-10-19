# Contributing to OpenMixer

Thank you for your interest in contributing to OpenMixer! This document outlines the conventions for both AI and human contributors.

## Quick Start

### For AI Agents

1. **Read `agents.md` first** - Contains complete setup instructions
2. **Check the plan** - `TODO.md` (the Before-Wave-7 list and the wave sections) and `ROADMAP.md`
3. **Create/update tasks** - Use `todowrite` tool to track progress
4. **Verify each change** - Run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`

### For Humans

1. Set up development environment (see `agents.md` and the Building section of `README.md`)
2. Fork the repository
3. Create a feature branch
4. Follow the clean-room doctrine

## Development Workflow

### AI Agent Checklist

**Before implementing:**
- [ ] Read `agents.md` for context
- [ ] Identify the wave and todo number
- [ ] Check ADRs for design decisions
- [ ] Verify no existing implementation exists

**While implementing:**
- [ ] Follow existing code patterns
- [ ] Add/expand tests
- [ ] Keep audio-thread code allocation-, lock- and syscall-free, and check it in tests where the harness allows (the `alloc_guard` module in `om-engine`, active in debug builds)
- [ ] Create evidence file: `.omo/evidence/task-<N>-openmixer-dj-app.*`

**After completing:**
- [ ] All tests pass
- [ ] `cargo clippy` clean with `-D warnings`
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

All changes — including those by AI agents and maintainers — land through a pull request. Never merge a branch directly or push to `main` or `stage`.

1. Work on a feature branch (`wave2-mixer`, `docs/...`, `fix/...`) cut from `stage`.
2. Open a PR into `stage` with `gh pr create --base stage`. The description must include:
   - **Summary:** what changed and why (link the Todo number from `TODO.md`).
   - **Changes:** key files/crates modules added or modified.
   - **Testing:** commands run and results (`cargo test --workspace`, `cargo clippy`, `cargo fmt --check`, `cargo deny check`, manual checks).
   - **Follow-ups / known gaps:** anything left undone.
3. Update `TODO.md`, `docs/progress.json` (then run `python3 tools/progress_heatmap.py`) and `docs/PROGRESS.md` in the same PR when task status changes.
4. Merge the PR (`gh pr merge --merge`) once the build and tests pass, so history stays traceable to a PR.
5. Release: batch `stage` into `main` with a PR (`--base main`); `rust.yml` runs on it as on every PR. Pushes alone never trigger CI.
6. Stack dependent work: when a PR builds on or touches the same files as an open PR, branch from that PR's branch and set `--base <that-branch>`. Keep the stack current by merging the parent branch into the child (merge commits, never force-push), so merging in order needs no rebase. After the parent merges, retarget the child with `gh pr edit --base stage`. Use plain merge commits, not squash, so stacked history stays intact.
7. Promote early: open a `stage` -> `main` PR at every major milestone (a finished Todo or feature group), not only at the end of a wave, so release PRs stay small and reviewable.

## Rust Standards & Memory Safety (required)

All code must follow idiomatic Rust (edition 2021, MSRV per `rust-version` in the workspace `Cargo.toml`). The Rust API Guidelines and the Rust reference are the reference. Unsafe or leaking code is not acceptable.

- **Unsafe policy:** every crate starts with `#![forbid(unsafe_code)]`, enforced by the workspace lint. The only exception is a dedicated FFI crate (`om-stretch`, a Rubber Band R3 binding behind a `Stretcher` trait, see `docs/spikes/DECISIONS.md`), where each `unsafe` block carries a `// SAFETY:` comment stating the invariant it relies on. Do not add `unsafe` to any other crate.
- **Ownership:** let ownership and borrowing express lifetimes. Use `Box`, `Arc` or `Rc` only where ownership is genuinely needed; no `Box::leak` or `mem::forget` to silence the borrow checker.
- **Idioms:** iterators over index loops where clearer, `?` for error propagation, enums with exhaustive `match`, `#[must_use]` where a result must not be ignored, `Option`/`Result` instead of sentinel values.
- **Errors:** `thiserror` in library crates, `anyhow` in the `openmixer` binary. Avoid `unwrap()` and `expect()` outside tests and documented invariants.
- **Concurrency:** the audio thread communicates through `rtrb` SPSC queues and `arc-swap` snapshots. No allocation, locks, channels that may block, or I/O on the audio thread. Document thread ownership in module docs.
- **Bounds & lifetime safety:** no unchecked indexing of untrusted data (use `get`); no `get_unchecked` outside the FFI crate.
- **Warnings:** code must compile warning-free; `cargo clippy --workspace --all-targets -- -D warnings` must pass. Do not add `#[allow(...)]` to pass the check unless the reason is written next to it.
- **Verification before opening a PR:** `cargo fmt --all --check`, `cargo clippy`, `cargo test --workspace` and `cargo deny check` must pass. Report the results in the PR's Testing section. Sanitizer and Miri runs for the FFI crate and concurrent code are planned.
- **Check current guidance:** when unsure about an API or idiom, consult the std docs, the crate's docs.rs page or the GPUI sources rather than reusing older patterns from memory.
- **Adopting crates:** well-established crates (cpal, symphonia, rusqlite, midir and similar) are adopted directly. Reserve spikes for genuine unknowns.

## Comments & Function Size (required)

- **Comments are short.** One line, stating *why* (not what). No paragraph or block comments describing what code does.
- **Long explanation = refactor.** If a function needs a long comment to be understood, split it into smaller, well-named functions instead.
- **Rule of 10 (goal, not a requirement):** aim for functions of about 10 lines or fewer. Exceed it only when splitting would hurt clarity.
- Prefer self-describing names over comments. Delete comments that restate the code.

## Modularity (required)

- **No monolithic files.** One focused responsibility per module; split anything that grows large or mixes concerns.
- **Use crates for boundaries.** Put reusable code in a workspace crate under `crates/om-<area>/` and depend on it where needed, instead of duplicating code or piling it into the `openmixer` binary.
- **Keep dependencies one-way.** Dependencies point toward `om-core`; the binary is the only composition root. Libraries must not depend on the binary or the UI crate and must be testable without a window.

## Architecture & Design Patterns (required)

Before starting any large implementation (a new subsystem, a cross-cutting feature, or a new crate):

1. Read `docs/DESIGN_PATTERNS.md` and review how similar code is already structured.
2. Reuse an existing pattern wherever it fits rather than inventing a new one.
3. If a different pattern would improve long-term maintainability (code growth, new features, scaling, testability), pick it deliberately and note the trade-off in the PR.
4. Document every new pattern you introduce in `docs/DESIGN_PATTERNS.md` (use its entry template) in the same PR, so later agents can find and reuse it.

## Token Efficiency & Knowledge Reuse (required)

Consult existing knowledge before computing or exploring. As the codebase grows, keep context small.

1. **Knowledge first:** check, in order, before searching or reading code: `graphify-out/` (query the graph first, e.g. `/graphify query`), any available knowledge base, then `docs/PROGRESS.md`, `docs/DESIGN_PATTERNS.md` and `TODO.md`.
2. **Read narrowly:** read only the needed line ranges or symbols, not whole files; prefer grep/glob to locate code. Do not re-read files already in context.
3. **Delegate broad searches** to a search subagent so only conclusions return to the main context.
4. **Keep the graph fresh:** after structural changes (new crates, moved modules) re-run graphify locally (`graphify-out/` is git-ignored). Document durable findings in `docs/` rather than rediscovering them.
5. **Don't recompute:** reuse existing results (test output, benchmarks, analysis) instead of re-running or re-deriving them.
6. **Keep outputs lean:** trim command output (`| tail`, `-q`), and keep PR descriptions and reports concise.

## Extensibility & User Freedom (required)

Users must always be able to modify OpenMixer with external plugins. Nothing may be locked down.

- **Extension points by default:** new features expose hooks for external plugins — audio pre/post-processing (per deck, channel and master), effects, analysis, library import/export, controller mappings, and UI/theming (colours, layouts, design tokens, skins).
- **No hardcoded restrictions:** don't hardcode behaviour, limits, themes or signal paths that a plugin could reasonably replace or extend. Prefer registries, traits and data-driven configuration (JSON/files) over fixed code paths.
- **Stable plugin API:** keep extension interfaces small, documented and versioned; don't break them silently. Plugins must not need to modify core source. The plugin API is planned (CLAP-first if and when needed).
- **No gatekeeping:** don't add signing, allow-lists or sandbox rules that stop users loading their own plugins (opt-in safety warnings are fine). Respect AGPLv3 obligations for plugins linked into the app.
- **PR check:** every PR must state whether it adds, changes or could block an extension point.

## Progress Files (required)

Every PR that completes or advances a task must update, in the same PR:
- `TODO.md` (tick items), `docs/progress.json` (status) and `docs/PROGRESS.md` (done/missing detail)
- the heatmap: `python3 tools/progress_heatmap.py` (CI's progress check fails if `docs/progress-heatmap.svg` is stale)

## CI Workflow Changes (required)

Any change to `.github/workflows/` must be tested locally with [`act`](https://github.com/nektos/act) before opening the PR.

1. Run the affected job(s): `act -j <job> [-W .github/workflows/<file>.yml]` (add `--container-architecture linux/amd64` if needed). List jobs with `act -l`.
2. Fix failures locally; don't push to find out.
3. Put the `act` command and result in the PR's Testing section. Note anything `act` can't cover (e.g. macOS/Windows runners, secrets).

## Code Style

### Naming Conventions

Standard Rust naming, enforced in review and by clippy:

- **Types, traits, enum variants:** `PascalCase`
- **Functions, methods, modules, fields:** `snake_case`
- **Constants and statics:** `SCREAMING_SNAKE_CASE`
- **Files:** `snake_case.rs`
- **Crates:** `om-<area>` (hyphenated in `Cargo.toml`, underscored in code)

### Formatting

Run `cargo fmt --all` before committing; CI runs `cargo fmt --all --check`. Formatting settings live in `rustfmt.toml`.

### File Organization

```
crates/
├── om-core/          # Parameters, state snapshots, commands (no I/O)
├── om-dsp/           # Filters, EQ, FX primitives
├── om-engine/        # Core audio engine (decks, mixer, FX routing)
├── om-audio/         # Device layer and latency monitor
├── om-analysis/      # Analysis pipeline (planned)
├── om-library/       # Library management (planned)
├── om-midi/          # MIDI input, mapping and feedback (planned)
├── om-ui/            # User interface, GPUI views (planned)
├── om-stems/         # Stem separation (planned)
└── openmixer/        # Binary: composition root, preferences
```

### Comment Requirements

See "Comments & Function Size" above. Each crate's `lib.rs` (or `main.rs`) starts with `#![forbid(unsafe_code)]` and a `//!` module doc of one line. Public items get at most a one-line `///` summary, and only when the name does not already say it.

## Testing

### Test Types

| Type | File Location | Description |
|------|--------------|-------------|
| Unit | `crates/<crate>/src/` (`#[cfg(test)]` modules) or `crates/<crate>/tests/` | Logic verification |
| Golden | `tests/golden/` (planned) | DSP correctness; reference vectors |
| Metric | `tests/metrics/` (planned) | Accuracy thresholds |
| Screenshot | `tests/screenshots/` (planned) | UI verification |

### Adding Tests

Tests use the built-in `cargo test` harness. Put unit tests in a `#[cfg(test)] mod tests` block next to the code, or integration tests in the crate's `tests/` directory.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptive_behaviour_name() {
        // assert! for preconditions and plain checks; compare floats with a tolerance
        assert!((0.1_f32 + 0.2 - 0.3).abs() < 1e-6);
    }
}
```

- Use a unique temporary directory per test so `cargo test` can run tests in parallel.
- Do not sleep to synchronise: use channels, condition variables or bounded waits.
- Tests that use real threads or timing are planned to be split out (e.g. `#[ignore]` with a dedicated CI job) so the fast run stays fast.
- Name tests for the behaviour they check; use `cargo test -- --list` to see what exists.

### Verification Commands

```bash
cargo build --workspace
cargo test --workspace                      # everything
cargo test -p om-engine                     # one crate
cargo test --workspace mixer                # by name filter
cargo test --workspace -- --list            # list tests
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo run -p openmixer -- --smoke           # startup check

# Leaks (Linux, requires valgrind)
valgrind --leak-check=full --error-exitcode=1 ./target/debug/openmixer --smoke
```

## Build Requirements

### Minimum System

- **Rust** toolchain pinned in `rust-toolchain.toml` (installed with rustup; `rustfmt` and `clippy` included)
- **Linux audio:** `pkg-config` and `libasound2-dev` (see the Building section of `README.md`)
- **cargo-deny** for `cargo deny check`
- **Audio libraries:** see `agents.md`

### Debug Build

```bash
cargo build
cargo run -p openmixer -- --smoke

# Memory leak detection (Linux)
valgrind --leak-check=full ./target/debug/openmixer --smoke
```

## Documentation

### Architecture Decision Records (ADRs)

When making architectural decisions:

1. Create `docs/adr/NNNN-title.md`
2. Follow ADR template:
   - Status (Proposed/Accepted/Rejected/Superseded)
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
- crates/om-engine/...

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
- Document asset provenance (see `docs/legal/asset-provenance.csv`)

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
- `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md` - original design (historical)
- `docs/adr/` - Architecture decisions
- `docs/legal/` - Legal documentation

**Plans:**
- `docs/superpowers/plans/` - early implementation plans (historical)
- `TODO.md` - Task list

---

*Thank you for contributing to OpenMixer!*
