# Build, test and constrained-machine instructions

## Dependencies

- **Rust toolchain:** install with `rustup`. The version is pinned in `rust-toolchain.toml` and selected automatically.
- **Linux audio:** `cpal` needs ALSA development headers and `pkg-config`:

```bash
sudo apt-get install libasound2-dev pkg-config libclang-dev libdbus-1-dev
```

`libclang-dev` is for bindgen (Rubber Band). The GPUI window needs the packages listed in the `check` job of `.github/workflows/rust.yml`. `libdbus-1-dev` is for the default `realtime-dbus` feature; `--no-default-features` builds without it.

- **WebKitGTK:** `om-ui-tauri` (the Tauri window, the web front end of ADR 0009) is a workspace member, so every workspace build on Linux needs `libwebkit2gtk-4.1-dev`; see "Tauri window (web front end)" below.
- **Cargo tools (for the lint and licence gates):** `cargo-deny` (CI installs it with `taiki-e/install-action`).
- GPUI platform dependencies (Linux) are documented in `openmixer-workbench/docs/spikes/gpui.md`.
- Crates added in the platform refresh: see [docs/dependencies.md](../dependencies.md).

## Build Commands

```bash
# Build the whole workspace
cargo build --workspace

# Release build
cargo build --release --workspace

# Run tests
cargo test --workspace

# Engine soak: 30 s paced mix (in cargo test --workspace); 10-minute gate: --ignored, 10 min wall time
cargo test -p om-audio --test soak
cargo test -p om-audio --test soak -- --ignored --nocapture

# Format, lint and licence gates (run all before opening a PR)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo deny check

# Startup smoke check
cargo run -p openmixer -- --headless --smoke

# Benchmarks (divan)
cargo bench -p om-engine
cargo bench -p om-analysis

# Optional features: CLAP hosting, real stem separation, ffmpeg decode fallback
cargo build -p openmixer --features clap
cargo test -p om-stems --features onnx
cargo test -p om-engine -p om-analysis --features om-engine/ffmpeg,om-analysis/ffmpeg
```

Sanitizer and Miri runs for concurrent code are planned; the current real-time check is `assert_no_alloc` in tests.

## Build setup and cost rule

- **sccache (Linux, optional):** to share compiled dependencies between worktrees, install `sccache` and put this in `~/.cargo/config.toml`:

```toml
[build]
rustc-wrapper = "sccache"
incremental = false
```

- **Cost rule for agents:** while working use `cargo check -p <crate>` and the touched crates' tests with `-j 4`. Run full clippy, workspace tests and `cargo deny check` once per PR, at the end. At most two parallel agents on the 12-core shared host.

## Working on a constrained machine (WSL)

- WSL crashes under heavy builds. Run `nice -n 19 env CARGO_BUILD_JOBS=3 cargo ...`, one cargo process at a time, never in the background, and never clippy and test in one command. Agents sharing a checkout serialise with `flock /tmp/openmixer-cargo.lock`.
- Agents on separate worktrees that share a `CARGO_TARGET_DIR` can see each other's output as fresh: touch the files you changed before testing, and check the new tests actually ran.
- Check `free -m` first. Do not build the gui crate unless more than 6 GB is free.
- GUI build (`openmixer-workbench/docs/spikes/gpui.md`): run with `WAYLAND_DISPLAY=` to force X11. For the link error `unable to find library -lxkbcommon-x11`, symlink `libxkbcommon-x11.so.0` into a shim directory and pass `RUSTFLAGS="-L <shim dir>"`; the proper fix is `sudo apt install libxkbcommon-x11-dev`. The gui crate tests need this too.
- `soak_short` (`crates/om-audio/tests/soak.rs`) can flake once under load; rerun it alone.
- Git corruption recovery: delete zero-byte files under `.git/objects`, run `git fetch --refetch origin`, then reset the branch to `origin/<branch>`.
- Never play audio; use the mock device. Screenshots only with `import -silent` (see `tools/screenshots.sh`), into temp dirs, at most 6 per task.
- Agents default to haiku; use sonnet for audio-thread or design-heavy work. Review once per phase. Push after every step.

## Tauri window (web front end)

The web front end of ADR 0009 (`om-ui-tauri`, the `gui-tauri` feature of `openmixer`, off by default). What it is and how it works: [docs/features/tauri-trial.md](../features/tauri-trial.md).

- **Linux packages:** `sudo apt-get install libwebkit2gtk-4.1-dev` (it brings GTK 3 and libsoup 3). `om-ui-tauri` is a workspace member, so `cargo check`, `clippy` and `test --workspace` need it even without the feature.
- **Node:** 22.12 or newer, or 24, with npm (Vite 8 needs 20.19 or 22.12 and up; Vitest 5 needs 22.12, 24 or 26 and up). Ubuntu's packaged Node is older. Dependencies are pinned in `web/package-lock.json`; install with `npm ci`.
- **Front end:** build it before the cargo build that should show it; the files in `web/dist` are embedded at compile time.

```bash
cd crates/om-ui-tauri/web
npm ci
npm run build      # copies resources/fonts into public/fonts, then vite build into dist/
npm test           # Vitest
npm run check      # svelte-check, warnings fail
cd -
```

- **Placeholder:** without `web/dist/index.html`, `crates/om-ui-tauri/build.rs` writes a one-line placeholder page there, so the crate and the workspace compile without Node. The window is then blank and `--smoke` fails with "the front end never called ready (is web/dist built?)". `npm run build` replaces it.
- **Cargo:**

```bash
cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device [--demo-library | --demo-scene=NAME]
cargo run -p openmixer --features gui-tauri -- --ui=tauri --mock-device --smoke
nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-ui-bridge
nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-ui-tauri
nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p openmixer --features gui-tauri   # needs a display and web/dist
```

- **CI:** the `web` job of `.github/workflows/rust.yml` (Linux) installs Node 22, runs `npm ci`, `npm run build`, `npm test` and `npm run check` in `web/`, then `xvfb-run -a cargo test -p openmixer --features gui-tauri`. The `check` job's apt line carries `libwebkit2gtk-4.1-dev` because its workspace-wide clippy and test build `om-ui-tauri` (against the placeholder page, without Node). The other jobs do not build the crate.
- **Bench:** `tools/ui-bench.sh <gpui|tauri> <seconds> [args...]` measures CPU and memory of a built binary (`OPENMIXER_BIN`, default `target/debug/openmixer`; the script never builds) and, for `tauri`, merges in the window's frame report. `tools/ui-bench.sh --self-test` checks its arithmetic. Fields and flags: the feature page.
- **Drive:** `python3 tools/ui-drive.py` presses, drags, scrolls and types into a built `--features gui-tauri` binary with XTest and checks each interaction against the engine (Linux X11 or WSLg; it moves the real pointer, so keep off the mouse for its two and a half minutes). Usage and limits: the feature page, "Driving the window".
- **WSL:** run the window with `WAYLAND_DISPLAY=` (the tests and the bench script set it). On Linux the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` unless it is already set, in `om_ui_tauri::prepare_environment`, which `gui_tauri::main` calls before any thread starts (ADR 0009: under WSL software rendering it cut CPU from 213% to 56%). If `npm` resolves to a Windows install, put a Linux Node first on `PATH` ([troubleshooting](../troubleshooting.md)). The window runs only with `--mock-device`.

## Test support

`crates/om-testing` holds the helpers the crates share in their tests: `wav` (fixture writers and a reader), `signals` (deterministic sines, noise and impulse trains), `mix` (a synthetic 120 BPM music-like track), `level` (dB, peak and RMS), `wait` (`wait_until` and `assert_eventually!`) and `fs::TempDir`. Reuse these instead of writing a new WAV writer, sine or poll loop. Waits poll for the condition they expect, with a timeout; never sleep a fixed time to wait for something to happen. It is a dev-dependency only: no shipped crate may depend on it.


## Testing

- TDD for engine and analysis logic
- Golden-vector fixtures (kept in the crates' test modules and `fixtures/`)
- Property tests (`proptest`) for parsers and search
- Metric tests for DSP accuracy; `divan` benchmarks (`cargo bench -p om-engine`, `-p om-analysis`) for the audio-thread and analysis hot loops
- Real-time tests: render the engine inside `assert_no_alloc`
- Screenshot tests for UI (planned)
