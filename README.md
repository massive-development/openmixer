# OpenMixer

A free, open-source DJ application: Linux first, with Windows and macOS support to follow.

## Mission

OpenMixer is a clean-room, legally original implementation of professional DJ software features. Our goal is to provide djay-Pro-class capability—multi-deck mixing, effects, sync, BPM/key analysis, stem separation, Automix, and controller support—without copying any copyrighted expression from existing applications.

## Clean-Room Statement

OpenMixer is built under a strict clean-room doctrine:

- **Functional features are free** - mixing workflows, deck layouts, BPM detection, sync mechanisms, and other operational concepts are methods of operation, not copyrightable expression
- **Expression is not copied** - no names, logos, artwork, icons, screenshots, manual text, sound packs, FX presets, or pixel-copying of screen layouts
- **No Mixxx code** - while we use the same open-source audio libraries (GPL-licensed), our implementation is entirely original
- **No proprietary assets** - AI model weights are downloaded on first use from official sources with licenses shown; no content is bundled

## Technology

OpenMixer is written in Rust with [GPUI](https://www.gpui.rs/) for the UI. The stack decision is in [ADR 0004](docs/adr/0004-stack-rust-gpui.md).


## Documentation

| Document | Purpose |
|----------|---------|
| [README.md](README.md) | Project overview (this file) |
| [agents.md](agents.md) | **AI Agent setup and implementation guide** |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [TODO.md](TODO.md) | Implementation task list |
| [docs/DESIGN_PATTERNS.md](docs/DESIGN_PATTERNS.md) | Design patterns in use and when to reuse them |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Per-task progress, gaps, and where to help |
| [docs/adr/INDEX.md](docs/adr/INDEX.md) | Architecture decisions |
| [docs/legal/](docs/legal/) | Legal documentation |

## Building

### Prerequisites

Install the Rust toolchain with [rustup](https://rustup.rs/). The toolchain version is pinned in `rust-toolchain.toml`; `rustfmt` and `clippy` are installed with it.

**Ubuntu/Debian:**
```bash
sudo apt-get install pkg-config libasound2-dev
```

GPUI system dependencies are documented in `docs/spikes/gpui.md`.

**macOS and Windows:** not runtime targets yet (Windows and macOS support is later). CI still builds them, and no extra system packages are listed yet.

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

# Engine soak: 30 s paced mix with a scripted DJ (also in cargo test --workspace)
cargo test -p om-audio --test soak
# 10-minute soak (Wave 2 gate, not in CI): takes 10 minutes of wall time
cargo test -p om-audio --test soak -- --ignored --nocapture

# UI gate: scripted DJ over the composition on a paced mock device, no display (8 s; 10-minute run is --ignored)
cargo test -p openmixer --test ui_gate
cargo test -p openmixer --test ui_gate -- --ignored --nocapture

# Format, lint and policy checks (same as CI)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check

# GPUI crate (outside the workspace; non-blocking in CI)
cargo fmt --manifest-path crates/om-ui-gpui/Cargo.toml --check
cargo clippy --manifest-path crates/om-ui-gpui/Cargo.toml --all-targets -- -D warnings
cargo deny --manifest-path crates/om-ui-gpui/Cargo.toml --config crates/om-ui-gpui/deny.toml check
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

The GPUI window is behind the `gui` feature, which is off by default and not built by the main CI matrix:

```bash
cargo run -p openmixer --features gui -- --mock-device
```

The `gui-check` job in `.github/workflows/rust.yml` builds the feature on Linux and is non-blocking. Nobody has yet looked at the window by eye.

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
├── om-core/      # Typed control surface (ParamId, Command), pulled engine snapshot, job queue, routing; no I/O
├── om-dsp/       # Filters, EQ, sweep filter, meters and crossfader curves
├── om-engine/    # Audio-thread engine: decks, mixer, FX racks, sync, quantizer and loops
├── om-audio/     # Device layer over cpal, a headless mock device, the output router
├── om-stretch/   # Rubber Band R3 key-lock time-stretch via FFI (the one crate allowed unsafe)
├── om-analysis/  # BPM, key, waveform and structure analysis; job service and SQLite cache
├── om-library/   # SQLite library, metadata ingest, iTunes and Rekordbox import, search, prep queue
├── om-midi/      # Controller maps, MIDI learn, feedback, trace replay; ALSA sequencer and mock backends
├── om-stems/     # Stem model manifests, licence-gated verified downloads (ONNX backend open)
├── om-plugins/   # In-process plugin API and registry
├── om-ui/        # UI logic without a window: themes, preferences, mixer, deck and library view models
├── om-ui-gpui/   # GPUI window (its own workspace, behind the openmixer `gui` feature)
└── openmixer/    # Binary and composition root: --smoke, --mock-device, settings, plugins
examples/plugins/gain-plugin/   # Example in-process plugin
fixtures/                       # Small test inputs: controller maps and traces, library exports
resources/themes/               # Built-in theme JSON
```

Real-time audio code lives in `om-engine` and `om-audio`. It must not allocate, take locks or make syscalls on the audio thread. The UI sends typed `Command::Param`s to the router through `CommandSink`. MIDI mappings and plugins use the string façade, which parses each id once at the router. Readers pull an `EngineSnapshot` built from the engine's atomics. `agents.md` lists what each crate owns and how to add a control id.

## Implementation Waves

![OpenMixer progress heatmap](docs/progress-heatmap.svg)

| Wave | Focus | Status |
|------|-------|--------|
| 1 | Foundation, build system, CI, test harness | ✅ Done |
| 2 | Audio engine (device, decks, mixer, FX, sync, MIDI input, soak gate) | ✅ Done; the debug panel (26a) is partial |
| 3 | Analysis pipeline | 🟡 Partial: the ONNX stems backend (19) is open |
| 4 | Library and data | 🟡 Done except play history, artwork persistence and FTS |
| 5 | User interface | 🟡 Partial: design tokens (25) and stem pads (30) open; decks, mixer and settings partial |
| 6 | Hardware I/O | 🟡 Partial: MIDI input and mapping are done (32 and 33 are in Wave 2); controller maps, HID and the mapping editor (34-36) are open |
| 7 | Intelligence (stems, Automix, sampler) | ⬜ Not started |
| 8 | Pro features (recording, broadcast, DVS, video) | ⬜ Not started |
| 9 | Release | ⬜ Not started |

The engine, analysis, library and MIDI work is verified headlessly on Linux; the Wave 5 UI is partial. Next: the Before-Wave-7 list in [TODO.md](TODO.md), then Wave 7.

See [TODO.md](TODO.md) for the task list and [docs/PROGRESS.md](docs/PROGRESS.md) for what each task entails, what is done, and what is missing.

## Important Notices

- **No Spotify integration** - Spotify's terms of service prohibit DJ mixing operations
- **No mobile/web app** - the current development focuses on desktop platforms
- **Stems are precomputed** - stem separation runs offline; real-time separation is not promised

## License

OpenMixer is released under the GNU Affero General Public License v3.0 (AGPLv3). See [LICENSE](LICENSE) for details.

## Contributing

For contributors, see [CONTRIBUTING.md](CONTRIBUTING.md). All contributions must follow the clean-room doctrine outlined in [docs/adr/0001-clean-room-doctrine.md](docs/adr/0001-clean-room-doctrine.md).

## Status

**Current State:** Works, verified headlessly on Linux: the audio engine (two decks, mixer, FX racks, sync, key-lock), analysis (BPM, key, waveform, structure), the library (ingest, import, search, prep queue), MIDI (ALSA sequencer, mapping and feedback), in-process plugins, and the GPUI app on Linux. Partial: the UI (decks, mixer, settings, debug panel) and stems (mock separator only). The GPUI window has never been viewed by eye. Windows and macOS support is later. **Next:** the Before-Wave-7 list in `TODO.md`, then Wave 7. See `docs/spikes/DECISIONS.md`.

**Known gaps, honestly listed:**

- Automix (Todo 38, Wave 7; the Automix panel waits on it) and the stem features (stem DSP, stem transitions, stem pads: Todos 37, 39, 30)
- The stems ONNX Runtime backend (Todo 19); the separator interface and the licence-gated downloads are done
- HID (Todo 35), the mapping editor UI (Todo 36) and plug-and-play controller maps (Todo 34)
- Dynamic plugin loading: plugins are in-process only; the loading choice is open in `docs/spikes/DECISIONS.md`
- Windows (WASAPI) and macOS (CoreAudio) runtime targets: later. The cpal device has not been run on a real ALSA or PipeWire device
- Visual verification of the GPUI window: the UI verification gate (Todo 31) runs at the logic layer, and nobody has checked the window by eye
- LookAndFeel (25): `om_ui::style` derives every control style from the tokens and the gpui controls draw from it; scroll bars, tables and toggles are not restyled yet, and the window has not been checked by eye. See "Styling a control" in `agents.md`. The debug panel's per-deck load and play (26a) is wired
- FFmpeg decode backend (Todo 8b, optional, open)
- Library: artwork is not stored (no artwork table); FTS is not used (LIKE only); there is no play-history table (only a `play_count` column)
- Saving engine hotcues back to the library is not confirmed
- Auto-enqueue after a library scan; playlist editing
- macOS: `soak_short` exceeds its xrun budget on hosted runners (that CI leg is non-blocking)
- The headphone-cue second stream is unproven (it failed on WSLg)
- The window-opening `--smoke` (behind `gui`) is not run in CI

---

Built with ❤️ for the DJ community.
