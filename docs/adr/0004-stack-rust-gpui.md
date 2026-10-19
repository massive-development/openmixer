# ADR 0004: Technology Stack - Rust with GPUI

## Status

Accepted (supersedes ADR 0002). Rust port at parity with the C++ baseline (Waves 1-6); Linux first; Windows/macOS porting phase and Waves 7-9 next

## Context

ADR 0002 chose JUCE 8.x (C++) as the application framework. The product is a
clean-room, AGPLv3, djay-Pro-class DJ application for Windows, macOS and Linux (Linux first; Windows and macOS follow in a porting phase)
(see ADR 0001 and ADR 0003). We are rewriting the application in Rust because
it is easier to audit: memory safety, a smaller `unsafe` surface, and one
toolchain via Cargo. We want a GPU-accelerated, component-style UI without a
webview. The C++ implementation is frozen at tag `cpp-final` and remains the
behavioural reference.

## Decision

We use **Rust** for the whole application, with **GPUI** as the UI toolkit.
The stack is a Cargo workspace (`om-core`, `om-dsp`, `om-engine`, `om-audio`,
`om-analysis`, `om-library`, `om-midi`, `om-ui`, binary `openmixer`).

Key choices:

| Area | Choice |
|---|---|
| Language / UI | Rust, GPUI `=0.2.2` from crates.io |
| Audio I/O | `cpal` for the engine output callback. `rodio` is acceptable only for non-critical audio such as library preview. |
| Decode / tags | `symphonia`; `lofty` |
| FFT / DSP | `realfft` / `rustfft`; own biquad, EQ and FX in `om-dsp` |
| Time-stretch / keylock | Rubber Band R3 via FFI in `om-stretch` (the one `unsafe` crate), behind a `Stretcher` trait so Signalsmith or a pure-Rust engine can be swapped in later. Decided by the author; no pure-Rust candidate passed the design bar in stereo in the Phase 1 spike (`docs/spikes/DECISIONS.md`) |
| MIDI | `midir`; HID later |
| Library | `rusqlite` (bundled SQLite) |
| RT plumbing | `rtrb` (SPSC), `arc-swap`, `assert_no_alloc` in tests |
| Stems | ONNX Runtime (`ort`), deferred to its wave |
| Errors / logging | `thiserror` in libs, `anyhow` in the binary, `tracing` |
| Plugins | Deferred; CLAP-first if/when needed |
| Tests | built-in `cargo test`, `proptest` for parsers/search, golden-vector fixtures carried over from C++ tests, `criterion` for DSP |
| Lints | `clippy -D warnings`, `cargo deny` (licences must stay AGPL-compatible), `cargo audit` |

### Why cpal rather than rodio

rodio is built on cpal. The case for cpal is not buffer control (rodio 0.23 has
`with_buffer_size`) but owning the render callback (allocation-free, driven by
an SPSC queue) and opening multiple streams for the headphone cue. The cue's
second stream failed on WSLg ("Invalid argument", cause not isolated), so it
is unproven on every backend; real-device latency and xrun numbers on
native Linux (Phase 2) and Windows and macOS (porting phase) are still to be measured. rodio remains acceptable
for non-critical audio such as library preview.

### GPUI go/no-go gate (closed on Linux)

GPUI is accepted. The first Phase 1 spike was a hard gate, now closed on Linux:

- `vulkaninfo --summary` on the dev WSL machine;
- a GPUI window with a custom-painted waveform on Linux (Windows and macOS move to the porting phase);
- a note on whether GPUI must be pinned to a git commit of Zed (resolved: `gpui =0.2.2` is on crates.io, so no git pin is needed).

Status: CLOSED on Linux. WSL2 X11 ran (window not visually seen, probably software-rendered; the Wayland backend panics on WSLg). A native-Linux run on a real GPU is nice-to-have and not blocking. Windows and macOS checks move to the porting phase (see `docs/spikes/DECISIONS.md`).

If the Linux gate fails, the fallback is to switch toolkit (iced first,
then Slint). We do not drop the platform or stop the project. WSL-only trouble
is a dev-loop issue and does not fail the gate on its own.

## Consequences

Positive:
- Memory-safe application code; `unsafe` confined to a few documented modules, with `#![forbid(unsafe_code)]` elsewhere.
- Single Cargo toolchain; cross-platform CI from the first crate.
- Headless-first: engine, analysis, library and MIDI are testable without a window.

Negative / risks:
- GPUI's release stability and licence (expected Apache-2.0, AGPL-compatible) are unverified beyond the Linux spike; Windows support is unverified until the porting phase.
- No pure-Rust time-stretcher passed the design bar in the Phase 1 spike. Decided: Rubber Band R3 via FFI behind a `Stretcher` trait, to be re-measured in the real engine.
- Windows (WASAPI) and macOS (CoreAudio) latency via cpal is measured in the porting phase.
- A full rewrite: the C++ tree stayed on the branch until the Rust port reached parity. It was then removed in one commit range; the `cpp-final` tag keeps it.

Licence impact: ADR 0002's dependency licences are re-checked under `cargo deny`. Rubber Band (GPL-2.0-or-later) is AGPL-compatible and is chosen; its `deny.toml` entry is enabled when `om-stretch` lands in Phase 2, not before.

## References

- Design: `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`
- Reference implementation: git tag `cpp-final` (main @ c6d2747)
- ADR 0001 (Clean-Room Doctrine), ADR 0002 (superseded), ADR 0003 (AGPLv3, no CLA)
