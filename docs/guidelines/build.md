# Build, test and constrained-machine instructions

## Dependencies

- **Rust toolchain:** install with `rustup`. The version is pinned in `rust-toolchain.toml` and selected automatically.
- **Linux audio:** `cpal` needs ALSA development headers and `pkg-config`:

```bash
sudo apt-get install libasound2-dev pkg-config libclang-dev libdbus-1-dev
```

`libclang-dev` is for bindgen (Rubber Band). The GPUI window needs the packages listed in the `check` job of `.github/workflows/rust.yml`. `libdbus-1-dev` is for the default `realtime-dbus` feature; `--no-default-features` builds without it.

- **Cargo tools (for the lint and licence gates):** `cargo-deny` (CI installs it with `taiki-e/install-action`).
- GPUI platform dependencies (Linux) are documented in `openmixer-workbench/docs/spikes/gpui.md`.
- Crates added in the platform refresh: see [docs/dependencies.md](docs/dependencies.md).

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
cargo run -p openmixer -- --smoke

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

## Test support

`crates/om-testing` holds the helpers the crates share in their tests: `wav` (fixture writers and a reader), `signals` (deterministic sines, noise and impulse trains), `level` (dB, peak and RMS), `wait` (`wait_until` and `assert_eventually!`) and `fs::TempDir`. Reuse these instead of writing a new WAV writer, sine or poll loop. Waits poll for the condition they expect, with a timeout; never sleep a fixed time to wait for something to happen. It is a dev-dependency only: no shipped crate may depend on it.


## Testing

- TDD for engine and analysis logic
- Golden-vector fixtures (kept in the crates' test modules and `fixtures/`)
- Property tests (`proptest`) for parsers and search
- Metric tests for DSP accuracy; `divan` benchmarks (`cargo bench -p om-engine`, `-p om-analysis`) for the audio-thread and analysis hot loops
- Real-time tests: render the engine inside `assert_no_alloc`
- Screenshot tests for UI (planned)
