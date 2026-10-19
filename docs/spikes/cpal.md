# Spike B: cpal output callback

THROWAWAY spike code in `spikes/cpal-out/` (own `[workspace]`; not built by the root workspace or Rust CI; `spikes/**/target/` is gitignored, `Cargo.lock` is committed). Date: 2026-10-18.

## Verdict

| Platform | Verdict | Basis |
|---|---|---|
| Linux / WSL2 (ALSA backend) | **BUILD-ONLY** (no usable device here; native-Linux device run open for Phase 2; render path is allocation-free) | Compiles with `libasound2-dev` + `pkg-config` (both present). Render path proven allocation-free under test. No ALSA hardware (`/dev/snd` has only `timer`), so the ALSA host cannot open a device here. |
| Linux / WSL2 (cpal PulseAudio host -> WSLg `RDPSink`) | **GO on WSL2** (native-Linux device run open for Phase 2) | Real stream ran 60 s at 256-frame request, zero xruns reported. Jitter is virtualised-audio jitter, not representative of hardware. Second stream failed to open (see below). |
| Windows (WASAPI) | **Porting phase** | Not run; not a gate. |
| macOS (CoreAudio) | **Porting phase** | Not run; not a gate. |
| Linux native (ALSA / PipeWire / JACK) | **Phase 2 (open)** | Not run on real hardware yet. |

Linux-first: latency/xrun targets for the Wave 2 soak need real hardware, which on Linux is a Phase 2 item (Windows and macOS are in the porting phase). The second-output-stream (headphone cue) question is an open Linux item for Phase 2: it failed on the only device available here, and the cause was not isolated.

## Silent-by-default rule (hard rule for all spikes/tests)

The render test is in-memory only. The device test (`device_opens_or_skips`) opens a real stream, which outputs **digital silence (all zeros)** by default: the callback still runs `render` and then zero-fills the buffer, so callback count / jitter / xrun measurement is unaffected. An audible tone is opt-in only, set by the USER: `OPENMIXER_AUDIBLE=1` (default off) enables a quiet tone with gain capped at 0.05 (-26 dBFS); the mid-run gain command is capped at half of that. Agents never set this variable. Correction (honest): my first 5 s and 60 s WSL runs, made before this rule existed, streamed a 440 Hz tone at gain 0.5 to the WSLg sink and were audible; the first 60 s table below is from that run. A later 4 s silent re-run gave comparable timing (660 callbacks, mean gap 6.067 ms, max 20.9 ms, 1 late, 0 xruns), so the metrics do not depend on audibility, but the 60 s numbers were not repeated silently.

## What was verified (and how)

- `cargo test` (debug), no device needed: `callback_does_not_allocate` renders 1000 buffers x 256 frames, stereo, 48 kHz via `render(out, channels, &mut State, &mut rtrb::Consumer<Cmd>)` under `assert_no_alloc`, with a `SetGain(0.25)` command pushed at buffer 500; asserts the final peak is ~0.25 (gain applied from 0.5). Passes.
- Mutation check: adding a `vec![0u8; 16]` inside `render` makes the test abort (`memory allocation of 16 bytes failed`, SIGABRT), so the guard is real. The global `AllocDisabler` is installed only under `debug_assertions` (the crate gates the type out otherwise).
- `device_opens_or_skips`: SKIPS with a printed note when there is no default output device **or** the default device gives no config. Important finding: on a box with no sound card, cpal's ALSA host still returns `Some(device)` for `default_output_device()` (`alsa:default`) and fails later at `default_output_config()` with "The requested audio device is not available". A bare `is_none()` check is therefore not enough to skip; the test checks both. Here it prints SKIP and passes.
- Build: `pkg-config --modversion alsa` = 1.2.11; cpal 0.18.2 compiles on toolchain 1.91.1 (repo `rust-toolchain.toml`). `cargo clippy --all-targets` and the release build are clean.

## Real-device run on WSL2 (cpal `pulseaudio` feature, WSLg PulseServer)

`cargo run --release --features pulse -- 60 --cue --pulse` (host `pulseaudio`, device `pulseaudio:RDPSink`, f32, 2 ch, 44100 Hz, `BufferSize::Fixed(256)` accepted, nominal period 5.805 ms).

**60 s metrics (audible run, before the silent-only directive; silent 4 s rerun matched):**

| Metric | Result |
|---|---|
| Callbacks in 60 s | 9926 (nominal 10336) |
| Mean inter-callback gap | 6.045 ms (nominal 5.805) |
| Max inter-callback gap | 15.395 ms (a 5 s run showed 19.3 ms) |
| Late callbacks (> 1.5x period) | 1 |
| Xruns reported via error callback | 0 |
| Other stream errors | 0 |

Caveats: the "Fixed(256)" request is passed to PulseAudio as a target buffer; the observed mean gap being 4% above nominal suggests the server delivers in its own chunking, so the callback size/rate is not confirmed to be exactly 256 frames. The gap measure is wall-clock between callbacks inside a virtualised RDP audio sink. No output latency was measured (cpal's `OutputCallbackInfo` timestamp was not used); this is only callback-timing jitter. Nobody listened to the tone (it was audible to the user by accident, see above).

## Second output stream (headphone cue)

Code path: `--cue` enumerates `host.output_devices()`, prefers a device different from the main one, else reuses the default device; tries `Fixed(256)`, then `BufferSize::Default`. Result here: only one device exists (`RDPSink`), so the same device was used and **both attempts failed with `Invalid input` ("Invalid argument") from the PulseAudio host while the first stream was playing**. I did not isolate whether this is a cpal pulse-host limit on concurrent streams, a WSLg RDP sink limit, or an issue in my probe. **Unproven: whether two simultaneous streams work on any backend.** This is the main risk to carry forward; the real test is two distinct physical devices (e.g. main outs + USB headphones) on each OS.

## rodio vs cpal (checked in rodio 0.23.0 and cpal 0.18.2 source from the cargo registry)

- cpal: you supply a push-style callback `FnMut(&mut [T], &OutputCallbackInfo)` to `build_output_stream`; the buffer size can be requested via `StreamConfig::buffer_size = BufferSize::Fixed(n)`.
- rodio: its playback feature depends on cpal (`playback = ["dep:cpal"]`); sources implement `trait Source: Iterator<Item = Sample>` (pull model, one sample at a time), mixed on rodio's own output thread.
- Correction to the ADR/design wording: rodio is not unable to control buffer size. `DeviceSinkBuilder::with_buffer_size(BufferSize)` exists and, when left at `Default`, rodio picks `Fixed` of about 50 ms (`sample_rate / 20`, rounded to a power of two). The remaining argument for cpal is the direct callback (you own the render loop, no iterator-per-sample interface, no hidden thread/mixer) and direct stream/device management, not buffer-size control. Whether rodio can open two devices at once was not checked.

## Run on native Linux (Phase 2); Windows and macOS in the porting phase

Prereqs: Rust 1.91.1 via `rust-toolchain.toml`. Linux: `sudo apt install libasound2-dev pkg-config`. Windows: MSVC build tools. macOS: Xcode command line tools.

```
cd spikes/cpal-out
cargo test -- --nocapture            # alloc test + device test (prints SKIP if no device)
cargo run --release -- 60            # 60 s single stream, prints metrics line
cargo run --release -- 60 --cue      # also probes a second stream (other device if any)
# WSL only, no ALSA hardware:  cargo run --release --features pulse -- 60 --cue --pulse
```

Run once with the default device, again with a pro interface if available, and again with `--cue` having a second output device present. The default run is silent (zeros) and measures callback timing only. For an audible check, YOU opt in with `OPENMIXER_AUDIBLE=1 cargo run --release -- 60 --cue` (quiet tone, gain <= 0.05; keep volume low): steady 440 Hz, level halves at 30 s, 880 Hz on the cue device. Fill in:

| Metric | Windows (WASAPI) | macOS (CoreAudio) | Linux native (ALSA/PipeWire) | WSL2 pulse (done) |
|---|---|---|---|---|
| Device / driver / sample rate | | | | RDPSink / WSLg / 44100 |
| `Fixed(256)` accepted? | | | | yes (accepted) |
| Callbacks in 60 s vs expected | | | | 9926 vs 10336 |
| Mean / max gap (ms) | | | | 6.045 / 15.395 |
| Late callbacks, xruns | | | | 1 / 0 |
| Second stream opens (other device)? | | | | n/a (single device) |
| Second stream opens (same device)? | | | | no (Invalid argument) |
| Audible glitches? | | | | not listened |
| Verdict GO / NO-GO | | | | GO on WSL2 (see Verdict) |

Also record, per OS: the actual period delivered if different from 256 (log `data.len() / channels` in the callback), and measured round-trip/output latency if you have a loopback method (not covered by this spike; the soak gate will use the real latency monitor).

## Open items (Linux first)

- [ ] Porting phase, Windows: run the commands above, fill the table (WASAPI shared mode: confirm whether `Fixed(256)` is honoured or the engine period is used).
- [ ] Porting phase, macOS: same (CoreAudio honours fixed buffer sizes within the device range; verify).
- [ ] Phase 2, Linux with real ALSA/PipeWire hardware: same.
- [ ] Phase 2, Linux: test `--cue` with two physical output devices; decide whether the headphone-cue design is viable on ALSA/PipeWire.
- [ ] Optional on WSL: install `alsa-utils` (`aplay -l`) to confirm no card, and `libasound2-plugins` to see whether the ALSA host can route to WSLg Pulse.
