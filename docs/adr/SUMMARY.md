# Architecture decisions (summary)

## ADR-0001: Clean-Room Doctrine
- **Features** are free (methods of operation)
- **Expression** is not (names, logos, artwork)
- No Mixxx code reuse (clean-room)
- All assets must be original

## ADR-0002: Technology Stack (superseded)
- Superseded by ADR-0004.

## ADR-0003: Licensing
- **Pure AGPLv3** (no Contributor CLA)
- Simple contribution process
- DCO sign-off required

## ADR-0004: Technology Stack - Rust with GPUI
- **Language / UI:** Rust (toolchain and MSRV 1.95), with GPUI for the UI: `gpui-pre` 0.3.10 imported as `gpui`, `gpui-kit` 0.8.1 and `gpui-component` (gate closed on Linux)
- **Audio I/O:** `cpal` for the engine output callback; `rodio` is acceptable only for non-critical audio such as library preview (not used so far)
- **Decode / tags:** `symphonia`; `lofty`
- **DSP:** `realfft` / `rustfft`; own biquad, EQ and FX in `om-dsp`
- **Time-stretch:** decided: Rubber Band R3 via FFI in `om-stretch` (`unsafe` confined to `ffi.rs`), behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `openmixer-workbench/docs/spikes/DECISIONS.md`)
- **MIDI:** `midir` on every platform (`MidirBackend` in `om-midi`; the earlier direct ALSA backend is gone, see `openmixer-workbench/docs/spikes/DECISIONS.md`); **Library:** `rusqlite` (bundled SQLite)
- **Resampling and loudness:** `rubato` (offline resampling, deck varispeed), `ebur128` (EBU R128 metering)
- **RT plumbing:** `rtrb` (SPSC), atomics for the pulled `EngineSnapshot`, `assert_no_alloc` in tests (`arc-swap` is not used)

## ADR-0005 to ADR-0008
- **0005 Experience-first product bar:** `docs/PRODUCT.md` is the bar; interface work is judged by looking at it
- **0006 Controller map provenance:** maps are written clean-room from the MIDI spec and the maintainer's own hardware traces
- **0007 CLAP hosting with clack:** accepted; `om-plugins` hosts CLAP plugins behind the `clap` feature, beside the C ABI (`clack-*` pinned `=0.2.0`)
- **0008 The Machined Faceplate supersedes E1 on look:** accepted; `openmixer-workbench/docs/design-system/DESIGN.md` is the look (warm matte neutrals, etched 1px lines, no glow, four muted deck hues each with its letter, signal red for playhead and record only, every number in mono). Phase 0 answers and the open owner decisions: `openmixer-workbench/docs/design/faceplate-decisions.md`

