# Rust Rewrite Phases 0–1: Scaffolding, Docs, Spikes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Rust workspace, CI, ADR and rewritten docs, then run the three go/no-go spikes (GPUI, cpal, time-stretch) that gate the rest of the rewrite.

**Architecture:** Rust tree lives at the repo root (`Cargo.toml`, `crates/`) beside the frozen C++ tree until Phase 5 deletes it. Crates depend toward `om-core`. Spikes are throwaway crates under `spikes/` outside the workspace's default members.

**Tech Stack:** Rust (stable, pinned in `rust-toolchain.toml`), cargo workspace, `cpal`, GPUI, `clippy`, `cargo-deny`, GitHub Actions (ubuntu/macos/windows).

**Spec:** `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`

## Global Constraints
- Baseline tag `cpp-final` (main @ c6d2747) is the behavioural reference; do not edit C++ sources in this plan.
- `#![forbid(unsafe_code)]` in every crate except a future dedicated FFI crate.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` must pass.
- Dependency licences must stay AGPLv3-compatible (`cargo deny`).
- Commits authored as `heyitsmass` (repo-local git config already set); end commit messages with the Co-Authored-By trailer from the session.
- Bulk doc rewrites go to `haiku` subagents; the main session reviews diffs.
- Never emit audible sound from tests, CI or agent runs: render to in-memory buffers; any real device stream writes digital silence; audible checks are opt-in (`OPENMIXER_AUDIBLE=1`, gain <= 0.05) and run by the user only.
- Spikes are labelled throwaway; keeping spike code is a new decision.
- Clean-room doctrine (ADR 0001) still applies to all docs and UI expression.

## Review Focus
- CI runners have no audio device or display: audio/UI tests must skip (not fail) when absent.
- Windows: path separators and CRLF must not break the parity/doc checks.
- A doc rewrite that silently drops a TODO item or a legal notice (clean-room, no Spotify, AGPL).
- `cargo deny` flags GPL-2.0-or-later deps (Rubber Band) as incompatible unless allow-listed deliberately.
- GPUI cannot open a window on headless CI: the gate is a manual run plus a compile-only CI check.

---

### Task 1: Workspace scaffold and `--smoke`
**Files:** Create `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `crates/om-core/{Cargo.toml,src/lib.rs}`, `crates/om-dsp/…`, `crates/om-engine/…`, `crates/om-audio/…`, `crates/openmixer/{Cargo.toml,src/main.rs}`, `crates/openmixer/tests/smoke.rs`; modify `.gitignore` (add `/target`).
**Interfaces:**
- Produces: `om_core::VERSION: &str`; binary `openmixer` accepting `--smoke` (prints `openmixer <version> smoke ok`, exit 0) and `--version`.
- [ ] **Step 1: Write failing test** `smoke_flag_exits_zero_and_prints_ok` in `crates/openmixer/tests/smoke.rs` using `std::process::Command` on `env!("CARGO_BIN_EXE_openmixer")`; assert status success and stdout contains `smoke ok`.
- [ ] **Step 2:** `cargo test -p openmixer` → FAIL (no workspace/binary).
- [ ] **Step 3:** Create workspace (resolver 2, `[workspace.package]` licence `AGPL-3.0-only`, shared lints table with `unsafe_code = "forbid"`), the five crates (each `forbid(unsafe_code)`, `om-core` exposes `VERSION = env!("CARGO_PKG_VERSION")`), and `main.rs` handling the two flags. Deps point toward `om-core` only.
- [ ] **Step 4:** `cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check` → PASS.
- [ ] **Step 5:** Commit `feat: rust workspace scaffold with --smoke`.

### Task 2: Rust CI and licence policy
**Files:** Create `.github/workflows/rust.yml`; modify `deny.toml`.
**Interfaces:** Consumes Task 1 workspace.
- [ ] **Step 1:** Write `deny.toml` allow-list: MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0, GPL-3.0, AGPL-3.0, plus an explicit commented entry for `GPL-2.0-or-later` (Rubber Band, only if the FFI fallback is chosen in Task 9). Run `cargo deny check licenses` → PASS.
- [ ] **Step 2:** Workflow `rust.yml`, triggers `pull_request` + `workflow_dispatch`, matrix `ubuntu-latest, macos-latest, windows-latest`; steps: checkout, `dtolnay/rust-toolchain` from `rust-toolchain.toml`, `Swatinem/rust-cache`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check` (Linux only), Linux job installs `libasound2-dev pkg-config`.
- [ ] **Step 3:** Push branch, open PR, confirm all three OS jobs green (`gh pr checks`). Expected: pass.
- [ ] **Step 4:** Commit `ci: rust matrix with clippy, fmt, deny`.

### Task 3: ADR 0004 supersedes ADR 0002
**Files:** Create `docs/adr/0004-stack-rust-gpui.md`; modify `docs/adr/0002-stack-juce-agpl.md` (Status → `Superseded by ADR 0004`), `docs/adr/INDEX.md`.
- [ ] **Step 1:** Write ADR 0004 from the design's Stack table (decision, why, consequences, GPUI gate and fallback, `cpp-final` reference).
- [ ] **Step 2:** Update 0002 status line and INDEX rows (0002 "Superseded", 0004 "Accepted").
- [ ] **Step 3:** Verify: `grep -n "Superseded" docs/adr/0002-stack-juce-agpl.md docs/adr/INDEX.md` shows both. Commit `docs: ADR 0004 Rust+GPUI`.

### Task 4: Parity inventory
**Files:** Create `tools/parity_inventory.py`, `docs/parity.md`, `tests/tools/test_parity_inventory.py`.
**Interfaces:** Produces `parity.md`: table `| Todo | Title | C++ test(s) | Rust test | Status |`, one row per `Todo N` in `TODO.md` at `cpp-final`.
- [ ] **Step 1:** Test `test_every_todo_has_a_row` parses `TODO.md` ids (`**Todo (\d+[a-z]?)`) and `docs/parity.md` rows; assert sets equal.
- [ ] **Step 2:** Run `python3 -I -m pytest tests/tools` → FAIL.
- [ ] **Step 3:** Implement `parse_todos(text: str) -> list[Todo]` and a writer; C++ test column filled by grepping `tests/` and `tools/` names mentioned in each todo; Rust column initially `—`, Status `todo`. Run it to generate `docs/parity.md`.
- [ ] **Step 4:** pytest → PASS. Commit `docs: parity inventory from cpp-final`.

### Task 5: Documentation rewrite (haiku subagents)
**Files:** Modify `README.md`, `agents.md`, `CONTRIBUTING.md`, `TODO.md`, `ROADMAP.md`, `docs/PROGRESS.md`, `docs/DESIGN_PATTERNS.md`, `docs/progress.json`; regenerate `docs/progress-heatmap.svg` via `tools/progress_heatmap.py`.
**Interfaces:** Consumes ADR 0004, `docs/parity.md`, the design.
- [ ] **Step 1:** Write `tools/check_docs_rust.py` that fails if any of those files contains `JUCE`, `CMake`, `cmake`, `ctest`, `g++`, `.cpp`, `.h ` outside fenced "history" notes, and fails if `AGPL`, `clean-room`, `Spotify` notices are missing from `README.md`. Run → FAIL.
- [ ] **Step 2:** Dispatch one `haiku` subagent per file group (README+ROADMAP; agents.md+CONTRIBUTING; TODO+PROGRESS+progress.json; DESIGN_PATTERNS), each given the design, ADR 0004 and its files. Instructions: keep structure and legal text, translate build/test/layout/patterns to Cargo/Rust/GPUI, reset statuses to Rust state (everything `todo` except scaffold), cite `cpp-final` for history.
- [ ] **Step 3:** Review each diff yourself; fix factual errors (commands must exist: `cargo build`, `cargo test`, `cargo run -p openmixer -- --smoke`).
- [ ] **Step 4:** `python3 -I tools/check_docs_rust.py` → PASS; regenerate heatmap; commit `docs: rewrite for Rust`.

### Task 6: Spike A — GPUI window (go/no-go)
**Files:** Create `spikes/gpui-window/{Cargo.toml,src/main.rs}`, `docs/spikes/gpui.md`.
- [ ] **Step 1:** Record `vulkaninfo --summary` output in `docs/spikes/gpui.md` (dev WSL).
- [ ] **Step 2:** Build a window that custom-paints a scrolling waveform (synthetic data, ≥60 fps counter overlaid) using GPUI as a pinned git revision of Zed (record the SHA) or crates.io if available. Compile-only check added to `rust.yml` (no display on CI).
- [ ] **Step 3:** Run on WSL and Windows (user runs Windows/macOS manually); record fps, build time, platform issues.
- [ ] **Step 4:** Write the decision in `docs/spikes/gpui.md`: GO / NO-GO per platform; on NO-GO name the fallback toolkit (iced then Slint) and stop for user input. Commit `spike: gpui window`.

### Task 7: Spike B — cpal output callback
**Files:** Create `spikes/cpal-out/{Cargo.toml,src/main.rs}`, `docs/spikes/cpal.md`.
**Interfaces:** Produces a render callback `fn render(out: &mut [f32], channels: usize)` plus an rtrb command ring.
- [ ] **Step 1:** Test (`cargo test`, skip with a printed note when `cpal::default_host().default_output_device()` is `None`): `callback_does_not_allocate` runs `render` under `assert_no_alloc` for 1000 buffers of 256 frames at 48 kHz with a parameter change arriving via the ring.
- [ ] **Step 2:** Run → FAIL (no render).
- [ ] **Step 3:** Implement a 440 Hz sine render with an SPSC-driven gain change; stream via cpal at 256 frames; log xruns/latency for a 60 s run.
- [ ] **Step 4:** Test → PASS; record ALSA/WASAPI/CoreAudio results in `docs/spikes/cpal.md` incl. whether a second output stream (headphone cue) can open. Commit `spike: cpal output`.

### Task 8: Spike C — time-stretch quality
**Files:** Create `spikes/stretch/{Cargo.toml,src/main.rs}`, `docs/spikes/stretch.md`.
- [ ] **Step 1:** Collect candidates (≥2 pure-Rust stretchers plus Rubber Band via FFI as reference); harness `stretch <in.wav> <ratio> <out.wav>` per candidate.
- [ ] **Step 2:** Offline metrics at ratios 0.92, 1.08: pitch error (cents) via autocorrelation, transient smear (onset envelope correlation vs original), CPU time per second of audio (`criterion`).
- [ ] **Step 3:** Write outputs to `spikes/stretch/out/` for the user's A/B listen; record numbers and a recommendation in `docs/spikes/stretch.md`. Acceptance: pitch error < 5 cents, one deck < 5% of a core. Commit `spike: time-stretch`.

### Task 9: Phase 1 decision record
**Files:** Create `docs/spikes/DECISIONS.md`; modify the design doc stack table.
- [ ] **Step 1:** Summarise GO/NO-GO for GPUI, cpal (incl. rodio-vs-cpal note), stretcher choice (pure Rust vs FFI; if FFI, enable the `GPL-2.0-or-later` entry in `deny.toml`).
- [ ] **Step 2:** Update the design's stack table to match; open PR; ask the user to approve before Phase 2 planning.

## Self-review
- Spec coverage: workspace/CI (T1–2), ADR (T3), parity inventory (T4), docs rewrite (T5), GPUI gate (T6), cpal/rodio (T7), stretch bar and Rubber Band licence (T8–9). Phases 2+ intentionally get their own plans after the spikes.
- Types: `om_core::VERSION`, `render(out, channels)` used consistently.
