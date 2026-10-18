# ADR 0004: Technology Stack - Rust with GPUI

## Status

Accepted (the GPUI choice is conditional on the Phase 1 GPUI go/no-go gate; see Decision)

Supersedes ADR 0002.

## Context

ADR 0002 chose JUCE 8.x (C++) as the application framework. The product is a
clean-room, AGPLv3, djay-Pro-class DJ application for Windows, macOS and Linux
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
| Language / UI | Rust, GPUI |
| Audio I/O | `cpal` for the engine output callback. `rodio` is acceptable only for non-critical audio such as library preview. |
| Decode / tags | `symphonia`; `lofty` |
| FFT / DSP | `realfft` / `rustfft`; own biquad, EQ and FX in `om-dsp` |
| Time-stretch / keylock | Spike first: pure Rust stretcher preferred; Rubber Band via thin FFI as fallback, kept in one `unsafe` crate |
| MIDI | `midir`; HID later |
| Library | `rusqlite` (bundled SQLite) |
| RT plumbing | `rtrb` (SPSC), `arc-swap`, `assert_no_alloc` in tests |
| Stems | ONNX Runtime (`ort`), deferred to its wave |
| Errors / logging | `thiserror` in libs, `anyhow` in the binary, `tracing` |
| Lints | `clippy -D warnings`, `cargo deny`, `cargo audit` |

### Why cpal rather than rodio

rodio is built on cpal, but its pull-based `Source` chain and managed output
thread do not give the sample-accurate, allocation-free, buffer-size-controlled
render loop, or the second headphone-cue stream, that decks and a mixer need.

### GPUI go/no-go gate (conditional)

GPUI is accepted provisionally. The first Phase 1 spike is a hard gate:

- `vulkaninfo --summary` on the dev WSL machine;
- a GPUI window with a custom-painted waveform on Linux and Windows;
- a note on whether GPUI must be pinned to a git commit of Zed (no authoritative crates.io status was found).

If the gate fails on a platform, the fallback is to switch toolkit (iced first,
then Slint). We do not drop the platform or stop the project. WSL-only trouble
is a dev-loop issue and does not fail the gate on its own.

## Consequences

Positive:
- Memory-safe application code; `unsafe` confined to a few documented modules, with `#![forbid(unsafe_code)]` elsewhere.
- Single Cargo toolchain; cross-platform CI from the first crate.
- Headless-first: engine, analysis, library and MIDI are testable without a window.

Negative / risks:
- GPUI's release stability, licence (expected Apache-2.0, AGPL-compatible) and Linux/WSL/Windows support are unverified until the gate passes.
- Pure-Rust time-stretch quality is unproven; FFI fallback is needed if it fails.
- Windows latency via cpal (WASAPI) must be measured.
- A full rewrite: the C++ tree stays on `main` until the Rust engine reaches parity. It is deleted only after Phase 5 passes.

Licence impact: ADR 0002's dependency licences are re-checked under `cargo deny`. Rubber Band (GPLv2+) remains AGPL-compatible if the FFI fallback is used.

## References

- Design: `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`
- Reference implementation: git tag `cpp-final` (main @ c6d2747)
- ADR 0001 (Clean-Room Doctrine), ADR 0002 (superseded), ADR 0003 (AGPLv3, no CLA)
- GPUI: https://www.gpui.rs
- cpal: https://github.com/RustAudio/cpal
