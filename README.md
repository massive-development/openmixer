# OpenMixer

A free, open-source DJ application for Windows, Mac, and Linux.

## Mission

OpenMixer is a clean-room, legally original implementation of professional DJ software features. Our goal is to provide djay-Pro-class capability—multi-deck mixing, effects, sync, BPM/key analysis, stem separation, Automix, and controller support—without copying any copyrighted expression from existing applications.

## Clean-Room Statement

OpenMixer is built under a strict clean-room doctrine:

- **Functional features are free** - mixing workflows, deck layouts, BPM detection, sync mechanisms, and other operational concepts are methods of operation, not copyrightable expression
- **Expression is not copied** - no names, logos, artwork, icons, screenshots, manual text, sound packs, FX presets, or pixel-copying of screen layouts
- **No Mixxx code** - while we use the same open-source audio libraries (GPL-licensed), our implementation is entirely original
- **No proprietary assets** - AI model weights are downloaded on first use from official sources with licenses shown; no content is bundled

## Rust Rewrite

OpenMixer is being rewritten in Rust with [GPUI](https://www.gpui.rs/) for the UI. The rewrite is governed by:

- [Design: Rust rewrite](docs/superpowers/specs/2026-10-18-rust-rewrite-design.md) - scope, architecture, phasing and gates
- [ADR 0004: Rust with GPUI](docs/adr/0004-stack-rust-gpui.md) - the stack decision (supersedes ADR 0002)
- [Parity inventory](docs/parity.md) - one row per TODO item, mapping each baseline test to its Rust replacement
- The `cpp-final` git tag (commit c6d2747) - the frozen C++ implementation, kept as the behavioural reference until Phase 5 passes. Inspect it with `git show cpp-final` or check it out with `git worktree add`.

## Documentation

| Document | Purpose |
|----------|---------|
| [README.md](README.md) | Project overview (this file) |
| [agents.md](agents.md) | **AI Agent setup and implementation guide** |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [TODO.md](TODO.md) | Implementation task list |
| [docs/DESIGN_PATTERNS.md](docs/DESIGN_PATTERNS.md) | Design patterns in use and when to reuse them |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Per-task progress, gaps, and where to help |
| [docs/parity.md](docs/parity.md) | Parity inventory: baseline tests and their Rust replacements |
| [docs/superpowers/specs/2026-10-18-rust-rewrite-design.md](docs/superpowers/specs/2026-10-18-rust-rewrite-design.md) | Rust rewrite design (authoritative) |
| [docs/adr/INDEX.md](docs/adr/INDEX.md) | Architecture decisions |
| [docs/legal/](docs/legal/) | Legal documentation |

## Building

### Prerequisites

Install the Rust toolchain with [rustup](https://rustup.rs/). The toolchain version is pinned in `rust-toolchain.toml`; `rustfmt` and `clippy` are installed with it.

**Ubuntu/Debian:**
```bash
sudo apt-get install pkg-config libasound2-dev
```

GPUI system dependencies are added after the Phase 1 spike.

**macOS and Windows:** no extra system packages are listed yet.

Optional, for the licence and ban policy check:
```bash
cargo install cargo-deny
```

### Build Steps

```bash
# Clone the repository
git clone https://github.com/massive-development/openmixer.git
cd openmixer

# Build the workspace
cargo build

# Run tests
cargo test --workspace

# Format, lint and policy checks (same as CI)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

### Running on WSL

GPUI renders through Vulkan, so the GUI on WSL depends on a working Vulkan driver as well as the display path. WSLg Vulkan can be fragile; check it with `vulkaninfo --summary` (from `vulkan-tools`) before debugging the app itself.

To run the GUI on Windows Subsystem for Linux (WSL):

**Windows 11 (WSLg):**
```bash
# Edit /etc/wsl.conf
[guiApplications]
enabled=true

# Restart WSL
wsl --shutdown
```

**Windows 10 or custom XServer:**
```bash
# Install and run VcXsrv or Xming on Windows
# Set DISPLAY environment variable
export DISPLAY=127.0.0.1:0

```

### Running

```bash
cargo run -p openmixer -- --smoke   # startup check
cargo run -p openmixer -- --version
```

The GUI is not built yet; the UI crate (`om-ui`) is planned for Phase 5.

### Debug Build

```bash
cargo build
cargo run -p openmixer -- --smoke

# Check for memory leaks (Linux, requires valgrind)
valgrind --leak-check=full ./target/debug/openmixer --smoke
```

## Architecture

A Cargo workspace. Dependency arrows point toward `om-core`, which has no I/O.

```
crates/
├── om-core/      # Currently exposes only VERSION; state snapshots and command enum planned
├── om-dsp/       # Empty scaffold; planned: filters, EQ, FX primitives, resampling helpers
├── om-engine/    # Empty scaffold; planned: decks, channel strips, crossfader, FX routing, sync, loops
├── om-audio/     # Empty scaffold; planned: device layer over cpal, latency monitor
└── openmixer/    # Binary: currently supports --smoke and --version; composition root and preferences planned

Planned: om-analysis and om-library (Phase 3), om-midi (Phase 4), om-ui (Phase 5), om-stems and om-plugins (their waves)
```

Real-time audio code lives in `om-engine` and `om-audio`. It must not allocate, take locks or make syscalls on the audio thread. UI and MIDI send commands to the engine; the engine publishes state snapshots back.

## Implementation Waves

![OpenMixer progress heatmap](docs/progress-heatmap.svg)

| Wave | Focus | Status |
|------|-------|--------|
| 1 | Foundation, build system, basic window | ✅ COMPLETE for the Rust scaffold (workspace, CI matrix, lint and deny config, `--smoke`) |
| 2 | Audio engine | ⏳ Not started in Rust (crate scaffold only; Phase 2) |
| 3 | Analysis pipeline | ⏳ Not started in Rust |
| 4 | Library & data | ⏳ Not started in Rust |
| 5 | User interface | ⏳ Not started in Rust |
| 6 | Hardware I/O | ⏳ Not started in Rust |
| 7 | Intelligence (stems, automix) | ⏳ Not started in Rust |
| 8 | Pro features | ⏳ Not started in Rust |
| 9 | Release | ⏳ Not started in Rust |

Waves are feature areas. The Rust rewrite proceeds in phases (0 scaffolding, 1 spikes, 2 engine, ...); see the design document. Phases 0 and 1 are in progress.

See [TODO.md](TODO.md) for the task list and [docs/PROGRESS.md](docs/PROGRESS.md) for what each task entails, what is done, and what is missing. Both are being rewritten for Rust.

## Important Notices

- **No Spotify integration** - Spotify's terms of service prohibit DJ mixing operations
- **No mobile/web app** - the current development focuses on desktop platforms
- **Stems are precomputed** - stem separation runs offline; real-time separation is not promised

## License

OpenMixer is released under the GNU Affero General Public License v3.0 (AGPLv3). See [LICENSE](LICENSE) for details.

## Contributing

For contributors, see [CONTRIBUTING.md](CONTRIBUTING.md). All contributions must follow the clean-room doctrine outlined in [docs/adr/0001-clean-room-doctrine.md](docs/adr/0001-clean-room-doctrine.md).

## Status

**Current State:** Rust rewrite. Phase 0 complete; Phase 1 spikes run — GPUI/audio/time-stretch gates await the user's Windows/macOS runs and listening test. See `docs/spikes/DECISIONS.md`. Wave 1 is complete for the Rust scaffold.

---

Built with ❤️ for the DJ community.
