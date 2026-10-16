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

## Documentation

| Document | Purpose |
|----------|---------|
| [README.md](README.md) | Project overview (this file) |
| [agents.md](agents.md) | **AI Agent setup and implementation guide** |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [TODO.md](TODO.md) | Implementation task list |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Per-task progress, gaps, and where to help |
| [docs/adr/INDEX.md](docs/adr/INDEX.md) | Architecture decisions |
| [docs/legal/](docs/legal/) | Legal documentation |

## Building

### Prerequisites

**Ubuntu/Debian:**
```bash
sudo apt-get install cmake g++ libasound2-dev libx11-dev libxrender-dev \
    libxrandr-dev libxcursor-dev libxi-dev libfreetype-dev \
    libfontconfig1-dev libcurl4-openssl-dev libgtk-3-dev \
    libwebkit2gtk-4.1-dev pkg-config
```

**macOS:**
```bash
brew install cmake
```

### Build Steps

```bash
# Clone the repository
git clone https://github.com/massive-development/openmixer.git
cd openmixer

# Configure
cmake -B build -DCMAKE_BUILD_TYPE=Release

# Build
cmake --build build

# Run tests
ctest --test-dir build
```

### Running on WSL

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
./build/openmixer_artefacts/Release/OpenMixer
./build/openmixer_artefacts/Release/OpenMixer --smoke   # headless-style startup check
```

### Debug Build

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build

# Check for memory leaks
valgrind --leak-check=full ./build/openmixer_artefacts/Debug/OpenMixer --smoke
```

## Architecture

```
src/
└── app/              # Application entry point and main window (implemented)

Planned: engine/ (Wave 2), analysis/ + stems/ (Waves 3, 7), library/ (4), ui/ (5), controllers/ (6)
```

## Implementation Waves

![OpenMixer progress heatmap](docs/progress-heatmap.svg)

| Wave | Focus | Status |
|------|-------|--------|
| 1 | Foundation, build system, basic window | ✅ COMPLETE |
| 2 | Audio engine | 🚧 IN PROGRESS |
| 3 | Analysis pipeline | ⏳ |
| 4 | Library & data | ⏳ |
| 5 | User interface | ⏳ |
| 6 | Hardware I/O | ⏳ |
| 7 | Intelligence (stems, automix) | ⏳ |
| 8 | Pro features | ⏳ |
| 9 | Release | ⏳ |

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

**Current State:** Wave 1 complete. Wave 2 (Audio Engine) in progress: device layer and deck engine done; mixer, MIDI and debug panel next.

---

Built with ❤️ for the DJ community.
