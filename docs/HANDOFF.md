# Session handoff

Read this first when resuming. It is a snapshot of 2026-10-20; re-check PR state with `gh pr list --state open`.

## 1. Project state

- OpenMixer is a DJ mixer app in Rust with GPUI for the UI. It is Linux-first and licensed AGPL.
- Wave 7 (stems, sampler, Automix) is built at the logic and view level only.
- Nothing has been verified by ear or by eye. The only visual check is screenshots from the mock device.
- No audio has been played. Development uses the mock device.

## 2. PR stack

Each PR targets the branch listed below it. Open PRs are drafts.

Already merged (verified with `gh pr view`):

- #73 `plan/wave7` into `main`: the Wave 7 plan.
- #74 `feat/wave7-foundation` into `main`: Phase F, typed ParamId, Command, EngineSnapshot, WorkerPool, errors, Services.
- #75 `feat/wave7-library` into `feat/wave7-foundation`: Phase L, play history, cue persistence, artwork, FTS5, LookAndFeel, debug panel.
- #76 `chore/screenshots`: screenshots and the regeneration script.
- #77 `fix/ui-layout`: layout and contrast fixes from the screenshot review.
- There is no #78.
- #79 to #89 were each merged into their base branch, not into `main`. All of that work now sits on `fix/ui-layout`: stems S1-S8, the stems review, sampler P1-P5, Automix A1-A6.

Open PRs:

- **#95** `fix/ui-layout` into `main`: the accumulated stack (Phase F follow-ups, library, stems, sampler, Automix). Mergeable.
- **#90** `fix/sampler-review`: base is `feat/wave7-automix-ui` (stale). It should be `fix/ui-layout`, but GitHub refuses the base change while the PR is part of a stack, so retarget it in the UI or after #91 and #92 are cleared. Its branch now contains `fix/ui-layout` (merge `ed5819f`), and it is mergeable. The tree is identical to the pre-merge review branch.
- **#91** `fix/automix-review`: base `fix/sampler-review`. Merge `a96d5e9` brings in the sampler review.
- **#92** `docs/handoff` (this document): base `fix/automix-review`.

PR stack, in merge order:

| PR | Branch | Base now | Target after its predecessor merges | Purpose |
|---|---|---|---|---|
| #95 | `fix/ui-layout` | `main` | `main` | Accumulated stack: Phase F follow-ups, library, stems, sampler, Automix |
| #90 | `fix/sampler-review` | `feat/wave7-automix-ui` (stale, should be `fix/ui-layout`) | `main` | Sampler review findings |
| #91 | `fix/automix-review` | `fix/sampler-review` | `main` | Automix review findings |
| #92 | `docs/handoff` | `fix/automix-review` | `main` | This document |

Merge order: #95 first, then #90, #91 and #92. Retarget each to `main` once the PR it sits on has merged. Their branches already contain `fix/ui-layout`, so the content is not lost.

Squash or not: a normal merge commit for #95 keeps the structure but leaves the wheels in `main` history. The wheels problem: `feat/wave7-stems` (#79) commits 26.4 MB of Python wheels under `onnx`, `numpy`, `ml_dtypes`, `protobuf` and `typing_extensions`. Commit `374fd2e` in #85 removes them and adds `*.whl` and `.venv` to `.gitignore`. Squash-merging #95 into `main` as one commit avoids the bloat. If the history must be kept, `git filter-repo` can strip the wheel paths before the merge. Either way, decide before merging #95.

## 3. Open work

Copied from the "Not done" list in PR #91 (`gh pr view 91`):

- 8. Crossfader assignment in `ChannelSnapshot` (om-core snapshot, engine mixer, StateId read) and the planner's use of it. Not started.
- 9. `LibraryQueue::pick`: lazy `has_grid`, shuffle-then-check, candidate cap. Not started.
- 10. Recent-filter relaxation (`recent_count = min(configured, pool/2)`) and a playlist wrap test. Not started.
- 12. Minor batch: `notify_user_input` allocation and dedupe; no-analysis `last_error` and negative cache; Armed re-arm loop back-off; `length_beats` 0 means the planner picks. Not started.
- 13. Deflake `seek_loop_and_hotcue_land_on_the_same_frames_as_plain_mode` (wait_until conversion and the 20x loop). Not started.
- 14. Docs: PROGRESS.md, progress.json, README/TODO wording, heatmap, `check_docs_rust.py`. Not started.
- Item 7, partial: no retry with another queue track (up to 3), and no tempo pre-filter in the queue.
- Item 6, limitation: the user-loaded stale-deck check uses track length only, because `DeckInfo` has no track id.

Other known gaps from the reviews:

- There is no Skip command for Automix.
- The Automix queue fails or suppresses when no candidate track is analysed.
- The Automix bar truncates "Next" below 900 px width.
- Echo-out assumes FX unit 0 and does not restore the unit afterwards (`crates/om-automix/src/moves.rs`).
- The gui crate tests need the `libxkbcommon-x11` shim (section 5).

FIRST ACTION for the next session: on `fix/automix-review`, run these one at a time:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

Workspace clippy and tests have not been run since the last Automix commits. `soak_short` (`crates/om-audio/tests/soak.rs`) may flake once under load. If it does, rerun it alone.

## 4. User-only verification

Nothing below can be checked from the sandbox.

- Export `htdemucs.onnx` with `tools/export_htdemucs.py`. Confirm the ModelContract: input `mix` `[1, 2, 343980]`, output `stems` `[1, 4, 2, N]`. The STFT-inside export may fail on complex ops. The STFT-outside variant (inputs `mix` and `z`) is tested only with fake models.
- Listening tests: stem seams and clicks, mute and solo, bass swap, key-lock with stems.
- Sampler by ear: latency, clicks, loop latch, gate.
- An Automix transition on real decks: phase alignment, EQ and bass swap, echo-out, and how cancel feels with real MIDI fader jitter.
- Real audio device and the headphone-cue second stream. This failed on WSLg.
- A real MIDI controller.
- The GPUI window by eye at 900x600 and smaller.
- The onnx CI job on a clean runner.
- Whether Automix should stay enabled across restarts.
- The macOS and Windows porting phase. Their CI legs were removed. Root causes are in `docs/spikes/DECISIONS.md`.

## 5. How to work

- WSL crashes under heavy builds. Always run `nice -n 19 env CARGO_BUILD_JOBS=3 cargo ...`.
- Run one cargo process at a time, never in the background. Do not chain clippy and test in one command.
- Check `free -m` first. Do not build the gui unless more than 6 GB is free.
- GUI build recipe (`docs/spikes/gpui.md`): run with `WAYLAND_DISPLAY=` to force X11. For the link error `unable to find library -lxkbcommon-x11`, make a shim directory with a symlink to `libxkbcommon-x11.so.0` and pass `RUSTFLAGS="-L <shim dir>"`. The proper fix is `sudo apt install libxkbcommon-x11-dev`.
- Git corruption recovery: delete zero-byte files under `.git/objects`, run `git fetch --refetch origin`, then reset the branch to `origin/<branch>`.
- Push after every step.
- Never play audio. Use the mock device only.
- Take screenshots only with `import -silent` (see `tools/screenshots.sh`), in temp dirs, and no more than 6 captures.
- Agents default to haiku. Use sonnet only for audio-thread or design-heavy work.
- Review once per phase.

## 6. Key docs

- `agents.md`: agent rules and global constraints.
- `docs/PROGRESS.md`: progress log.
- `TODO.md`: the Before-Wave-7 list (section "Before Wave 7").
- `docs/spikes/DECISIONS.md`: decisions, including porting root causes.
- `docs/superpowers/plans/2026-10-19-wave7-foundation-and-features.md`: the Wave 7 plan.
- `docs/screenshots/REVIEW.md`: screenshot review.
