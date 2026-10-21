# Session handoff

Read this first when resuming. It is a snapshot of 2026-10-20; re-check PR state with `gh pr list --state open`.

## 1. Project state

- OpenMixer is a DJ mixer app in Rust with GPUI for the UI. It is Linux-first and licensed AGPL.
- Wave 7 (stems, sampler, Automix) is built at the logic and view level. `main` holds all of it through #95.
- Nothing has been verified by ear or by eye. The only visual check is screenshots from the mock device.
- No audio has been played. Development uses the mock device only.
- Workspace fmt, clippy and tests passed on the review branches (#96 to #98) before this snapshot. They must be rerun after #98 merges into `main`.

## 2. PR state

Merged (verified with `gh pr list --state all`):

- #73 (plan) and #74 (Phase F) reached `main` directly.
- #75, #76 and #77 were merged into their base branches (`feat/wave7-foundation`, `feat/wave7-library`, `chore/screenshots`), not into `main`.
- #79 to #91 were merged into their base branches. #92 (this document's previous version) merged into `fix/automix-review`.
- #95 `fix/ui-layout` into `main` (2026-10-20, merge commit `c84dd7e`). It carries the whole stack above into `main`, including stems, sampler and Automix.

Open PRs, all into `main`:

| PR | Branch | Content |
|---|---|---|
| #96 | `fix/automix-review` | Sampler and Automix review fixes; queue shuffles before the grid check and checks lazily, capped at 50; recent window is at most half the pool and relaxes when nothing is left |
| #97 | `fix/decoder-stall` | Test-harness fix, not an engine bug: the render wait treated a ring parked at end of stream as unhealthy. Stress test added and ignored |
| #98 | `fix/automix-xfader` | Automix respects each channel's crossfader assignment instead of deck parity |

`origin/fix/automix-xfader` contains the commits of #96 and #97, so merging #98 brings in all three. Rerun the checks in section 3 after that merge.

## 3. Open work

Automix (from the review findings in #91 and #96):

- Item 6, remaining: `DeckInfo` has no loaded-track id. The user-loaded stale-deck check uses track length only. Add the loaded-track id to the host deck info.
- Item 7, remaining: the planner does not retry with another queue track (up to 3 times), and the queue has no tempo pre-filter for explicit styles.
- Item 12, minor batch:
  - `notify_user_input` notices need filtering, dedupe and a cap.
  - Idle with no analysed candidate sets `last_error` once, and the queue needs a negative-lookup cache of about 2 s. Today the queue fails or suppresses when no candidate is analysed.
  - Armed `deck_ok` failure needs a back-off.
  - `AutomixPrefs.length_beats` of 0 should mean the planner picks the length.
- No Skip command for Automix.
- Crossfader ramp dips on a centred crossfader when the channel is reassigned.
- A refusal does not retry the track.
- Echo-out assumes FX unit 0 and does not restore it afterwards (`crates/om-automix/src/moves.rs`).
- The Automix bar truncates "Next" below 900 px width.

Docs:

- Item 14: make PROGRESS.md, progress.json, README.md and TODO.md wording consistent. Check with `python3 -I tools/check_docs_rust.py`.

FIRST ACTION for the next session: after #98 merges, on `main`, run these one at a time:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

`soak_short` (`crates/om-audio/tests/soak.rs`) may flake once under load. If it does, rerun it alone.

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
- GUI build recipe (`docs/spikes/gpui.md`): run with `WAYLAND_DISPLAY=` to force X11. For the link error `unable to find library -lxkbcommon-x11`, make a shim directory with a symlink to `libxkbcommon-x11.so.0` and pass `RUSTFLAGS="-L <shim dir>"`. The proper fix is `sudo apt install libxkbcommon-x11-dev`. The gui crate tests need this shim.
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
