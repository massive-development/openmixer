# Spike C: time-stretch (keylock) quality

THROWAWAY spike code in `spikes/stretch/` (own `[workspace]`; not built by the root workspace or Rust CI; `Cargo.lock` is committed, `target/` and `out/` are gitignored). Date: 2026-10-18. Everything is **offline**: WAV files in, WAV files out. No audio device is opened and nothing is played by the harness, tests, or the agent run. The generated WAVs in `spikes/stretch/out/` are for the user to listen to.

## Verdict

**Provisional, not final: the numbers narrow the field but cannot close this spike; the user's A/B listen on real music is still required.** The design bar is judged on STEREO decks (measured, L/R correlated at 7.5 ms (R = 0.9x copy of L delayed 331 samples), whole-file render, wall-clock, +-20% noise on a shared WSL2 VM):

| Candidate | Pitch < 5 cents | Transients (attack rise vs 2.2 ms orig) | Stereo CPU per deck (< 5% of a core) | Overall on numbers |
|---|---|---|---|---|
| `wsola` (pure Rust, MIT) | pass (< 1 cent) | good (2.6 / 4.6 ms) | **FAIL: 5.3% at 1.08, 6.2% at 0.92** (mono 3.1-3.8%) | fails the CPU bar as shipped |
| `timestretch` `WideKeylock` (pure Rust, MIT) | pass (0 cents) | **poor (12-15 ms)** | **5.0% at 0.92 (4.98%, marginal), FAIL 5.7% at 1.08** | fails transients; CPU marginal/over |
| `timestretch` `Keylock` / batch (pure Rust) | **fail at 0.92 (5.6 cents sine)** | poor (13-15 ms) | pass (2.1-3.1%) | fails pitch and transients |
| `signalsmith-stretch` (MIT, C++ via `cc`) | pass (< 1 cent) | fair (3.7-7.2 ms) | pass (0.8-1.0%) | passes every measured bar |
| Rubber Band 4.0.0 R3 (`ENGINE_FINER`, GPL-2.0-or-later) | pass (<= 3.4 cents strict, 0 on sine) | fair (5.4-7.1 ms) | **marginal / FAIL: 5.6% at 0.92, 4.9% at 1.08** | borderline on CPU |
| Rubber Band 4.0.0 R2 (default options) | **anomalous: ~20 cents on a steady sine** | crisp (2.7-3.2 ms) but unreliable | pass (1.5-2.3%) | not usable as evidence |

Consequences, stated for Task 9:

1. **No pure-Rust candidate passes the design bar as measured.** The only pure-Rust options that keep pitch (`wsola`, `timestretch-wide`) exceed 5% of a core per stereo deck here; the one that is cheap (`timestretch` Keylock) fails pitch. `wsola`'s crate is an unoptimised brute-force search, so a cheaper config (smaller search range, larger hop) or our own optimisation might bring it under 5%, but that is untested, and a 4-deck mixer needs margin well under 5% each. By the design's own rule ("pure Rust is accepted only if ... one deck fits in a small share of one core") pure Rust is not yet accepted. Whether `wsola` sounds good enough to be worth optimising is a question for ears.
2. **Licence is not an argument against Rubber Band.** The design states Rubber Band's GPL-2.0-or-later is AGPLv3-compatible, already used in the C++ build, and licence-acceptable. Needing the `GPL-2.0-or-later` line in `deny.toml` is therefore a one-line change, not a reason to prefer Signalsmith. (That change remains a Task 9 decision.)
3. **Signalsmith is not pure Rust.** `signalsmith-stretch` is C++ compiled by `cc`, i.e. FFI. Picking it contradicts the design's pure-Rust-first stance and would make FFI the default path rather than the fallback, with a C++ toolchain requirement on every OS (MSVC on Windows, Xcode tools on macOS; the Rubber Band route has the same C++ build cost, plus `bindgen`/libclang through `rubberband-sys`). I only built it on Linux here; the Windows/macOS build cost is unmeasured.
4. **Only Rubber Band R3 is a trustworthy Rubber Band reference.** R2's ~20-cent drift on steady tones (see findings) makes every R2 comparison, including its crisp transient numbers, unreliable. R3 is clean on pitch, but at 4.9-5.6% stereo CPU it sits on the bar.
5. **Measured-best all-round is Signalsmith** (cheapest, passes pitch/CPU, fair transients), with the FFI and C++ build caveats of point 3. Whether to take an FFI default (Signalsmith or Rubber Band R3) versus invest in making a pure-Rust stretcher fast enough is the Task 9 decision; I do not make it. I have not heard any output.

## Candidates considered

Searched `cargo search` for: `time stretch`, `phase vocoder`, `wsola`, `signalsmith`, `pitch shift`, `psola`, `rubberband`. Licences are from `cargo info`. "AGPL-3.0-only compatible" = permissive (MIT) or GPL-2.0-or-later/GPL-3.0 per the design doc.

| Crate | Version | Licence | Language | Evaluated? | Notes |
|---|---|---|---|---|---|
| `wsola` | 0.2.0 | MIT | pure Rust | **yes** | Streaming push/pull WSOLA, mono/interleaved f32, tempo 0.25-4. Small, new crate (from a podcast tool repo); rust-version 1.87 (repo toolchain is 1.91.1, fine). Defaults tuned for speech (15 ms hop, 12 ms search). |
| `timestretch` | 0.15.0 | MIT | pure Rust (`#![forbid(unsafe_code)]`) | **yes** | EDM-targeted; real-time pull engine (`Engine`, lock-free controller, allocation-free `process`) with profiles `Tape`, `Keylock` (band split at 120 Hz + SOLA), `WideKeylock` (phase vocoder), plus batch `stretch()`. Heavy API surface (beat grids, analysis, loudness). Depends on `rustfft`, `ebur128`, `serde`. rust-version 1.85. |
| `signalsmith-stretch` | 0.1.3 | MIT | C++ (Signalsmith Stretch, built by `cc`) | **yes** | Thin wrapper; `preset_default`, `process`/`exact`/`flush`, `set_transpose_factor`. Not pure Rust, but permissive, so no `deny.toml` change. Would live in the same one-`unsafe`-crate boundary as a Rubber Band fallback. |
| `rubberband` / `rubberband-sys` | 0.1.5 / 0.1.4 | GPL-2.0-or-later | C++ (vendored Rubber Band 4.0.0 single-file build via `cc` + `bindgen`) | **yes (reference)** | `rubberband-sys` vendors the sources, so no system library or `_deps/` checkout is needed (the repo `_deps/` has no `rubberband-src`; only an unrelated copy in a Windows recycle bin on `/mnt/e`, not used). Needs libclang for `bindgen` (present here). |
| `cute-dsp` | 0.0.41 | MIT | Rust | no | Very young (0.0.x); not evaluated. |
| `pitch_shift` | 2.1.0 | MIT | Rust | no | Phase-vocoder pitch shifter, pitch-oriented API; not evaluated. |
| `phase_vocoder`, `pvoc`, `rocoder` | 0.1.0 / 0.1.7 / 0.4.0 | not checked | Rust | no | Generic/experimental phase vocoders (`rocoder` is live-coding oriented); not evaluated. |
| `ssstretch` | 0.1.0 | not checked | C++ binding | no | Second Signalsmith binding; chose the more established `signalsmith-stretch` 0.1.3. |
| `rodio-wsola` | 0.2.0 | not checked | Rust | no | rodio adapter around `wsola`; irrelevant (we use cpal). |
| `kithara-stretch` | 0.0.1-alpha5 | not checked | Rust | no | Alpha adapter crate. |
| `tdpsola` | 0.1.0 | AGPL-3.0 | Rust | no | TD-PSOLA is for monophonic voice pitch/formants, not polyphonic music; SPDX id is `AGPL-3.0` not `AGPL-3.0-only`. Rejected on fit. |

Only the first four rows were built and measured. "Not evaluated" means metadata only; I do not claim anything about their quality.

## Method

Harness: `spikes/stretch/` (`cargo build --release`; run from that directory). Commands: `stretch <in.wav> <ratio> <out.wav> <engine>` (engine required; mono or stereo WAV), `stretch gen`, `stretch eval`, `stretch engines`. `ratio` is the DJ tempo factor: `1.08` = 8% faster, output length = input / ratio, pitch preserved. `hound` for WAV (16-bit mono out). Engines (44.1 kHz, single thread; quality metrics run mono, CPU also run stereo):

- `wsola`: wsola 0.2.0 default config, streamed in 512-sample chunks.
- `timestretch-keylock` / `timestretch-wide`: the real-time `Engine` with `EngineProfile::Keylock` / `WideKeylock`, 512-frame callbacks, **no pre-analysis supplied** (online transient detection).
- `timestretch-batch`: batch `stretch()` (computes whole-file pre-analysis itself).
- `signalsmith`: `preset_default`, `exact()` over the whole buffer.
- `rubberband-rt`: Rubber Band 4.0.0 real-time mode, **default options** (R2 engine), 512-sample blocks, start-pad/delay compensated. `rubberband-rt-r3`: same with `ENGINE_FINER`. `rubberband-offline`: offline study+process (R2), the non-real-time reference. `rubberband-rt-smoothing`, `rubberband-rt-mixed`: R2 with `SMOOTHING_ON` / `TRANSIENTS_MIXED|SMOOTHING_ON` (added after the finding below). All with `THREADING_NEVER`.

Test material (generated, deterministic, no real music): `chord` = A3+C#4+E4 (220, 277.18, 329.63 Hz, each with a 30% second harmonic), 8 s; `drums` = 120 BPM kick sweeps every beat, noise snare on 2 and 4, 8th hats, one 0.5 ms click per bar, 8 s; plus a plain 440 Hz sine, 8 s, for the pitch metric only. Ratios 0.92 and 1.08.

Metrics (unit-tested on known signals: `cargo test --release`; silent):

- **Pitch error (cents).** Hann-windowed 65536-pt FFT of the centre of the signal. Two estimators, both reported as output vs the same measurement on the unstretched input (cancels bin bias): (a) strict magnitude peak with parabolic interpolation on log-magnitude (max over the 3 chord fundamentals; single peak for the sine); (b) power-weighted centroid within +-3% of each fundamental (robust to amplitude-modulation sidebands that a phase vocoder can add around a steady partial). Tests: a 1% shifted sine reads 17.2 +- 1 cents; an off-bin 300.37 Hz tone reads within 1 cent.
- **Transient smear.** (a) Onset-envelope correlation: RMS envelope (1.5 ms window, 0.5 ms hop), sqrt-compressed, half-wave-rectified difference, 5 ms smoothed; the original's onset envelope is time-warped by 1/ratio and compared with the output's by normalised cross-correlation, maximised over +-20 ms lag (lag reported separately; it is latency, not smear). 1.0 = identical. (b) Attack rise time: mean over detected drum onsets of the time from the RMS envelope falling below 10% of the local peak to the peak; original = 2.2 ms. Rise time is the more sensitive smear indicator; NCC is dominated by the loud kicks.
- **CPU.** `std::time::Instant` wall-clock for the whole 8 s mono render (includes engine construction), 1 warm-up + 5 timed runs, median, divided by the 8 s of audio. Reported as ms per audio-second; ms / 10 = % of one core. Release build on this WSL2 box (12 logical CPUs, not otherwise idle-controlled), so +-20% noise is plausible. Peak memory: not measured.

### Deviations from the brief

- **Pitch estimator:** the brief said autocorrelation; I used an FFT peak (parabolic interpolation) plus a power-weighted centroid, because the chord is polyphonic (autocorrelation does not give three fundamentals).
- **CPU timing:** the brief said `criterion`; I used `std::time::Instant` (1 warm-up + 5 runs, median) over whole-file renders.
- **Channel coverage:** the first pass was mono only; stereo CPU was added in the review-fix round. Pitch and transient metrics are still mono-only (stereo outputs are only tested for length, finiteness and non-silent channels).
- **Rubber Band** is vendored through `rubberband-sys` (the brief allowed an existing -sys crate) rather than from `_deps/`, which has no `rubberband-src`.
- Peak memory (optional) not measured.

### Metric limits

- Pitch: one 65536-sample (1.49 s) window at the centre of the output, so it misses pitch drift or glitches elsewhere in the file; steady synthetic tones only.
- Smear: the rise-time number is a mean over detected onsets with a +-30 ms peak pick around each, so one wrong pick or a mix of good and badly smeared hits is hidden, and neighbouring events within 30 ms can contaminate it; it is noisy at the 0.5 ms envelope resolution, and NCC is dominated by the loud kicks.

## Results

All numbers below are output of `cargo build --release && ./target/release/stretch-spike eval` (also saved to `spikes/stretch/out/results.md`, not committed). Pitch and transient tables are mono; the CPU table has mono and STEREO (L = material, R = 0.9x copy of L delayed 331 samples = 7.5 ms, so correlated at 7.5 ms, not decorrelated) columns. Design bar: pitch < 5 cents; one deck < 5% of a core, **judged on the stereo column**.

#### Pitch

| engine | tempo | sine 440 Hz, FFT peak (cents) | chord, centroid (cents, max of 3) | chord, FFT peak (cents, max of 3) |
|---|---|---|---|---|
| wsola | 0.92 | 0.01 | 0.83 | 0.87 |
| wsola | 1.08 | 0.01 | 0.88 | 0.95 |
| timestretch-keylock | 0.92 | 5.56 | 3.75 | 12.48 |
| timestretch-keylock | 1.08 | 0.10 | 2.61 | 24.41 |
| timestretch-wide | 0.92 | 0.00 | 0.71 | 0.00 |
| timestretch-wide | 1.08 | 0.00 | 0.18 | 0.00 |
| timestretch-batch | 0.92 | 5.56 | 4.96 | 20.99 |
| timestretch-batch | 1.08 | 0.10 | 3.20 | 27.11 |
| signalsmith | 0.92 | 0.00 | 0.82 | 0.89 |
| signalsmith | 1.08 | 0.00 | 0.20 | 0.22 |
| rubberband-rt | 0.92 | 19.77 | 23.13 | 39.74 |
| rubberband-rt | 1.08 | 22.92 | 11.56 | 37.25 |
| rubberband-rt-r3 | 0.92 | 0.00 | 1.20 | 3.44 |
| rubberband-rt-r3 | 1.08 | 0.00 | 0.00 | 0.00 |
| rubberband-offline | 0.92 | 13.23 | 6.25 | 28.45 |
| rubberband-offline | 1.08 | 20.32 | 8.31 | 35.32 |
| rubberband-rt-smoothing | 0.92 | 9.98 | 1.86 | 8.01 |
| rubberband-rt-smoothing | 1.08 | 1.51 | 3.34 | 36.72 |
| rubberband-rt-mixed | 0.92 | 0.00 | 0.00 | 0.00 |
| rubberband-rt-mixed | 1.08 | 0.00 | 0.00 | 0.00 |

#### Transients

| engine | tempo | onset NCC | best lag (ms) | attack rise orig -> out (ms) |
|---|---|---|---|---|
| wsola | 0.92 | 0.920 | +11.0 | 2.2 -> 2.6 |
| wsola | 1.08 | 0.920 | +12.0 | 2.2 -> 4.6 |
| timestretch-keylock | 0.92 | 0.837 | -3.0 | 2.2 -> 14.9 |
| timestretch-keylock | 1.08 | 0.853 | +2.5 | 2.2 -> 12.6 |
| timestretch-wide | 0.92 | 0.836 | -2.5 | 2.2 -> 15.3 |
| timestretch-wide | 1.08 | 0.830 | +1.0 | 2.2 -> 12.0 |
| timestretch-batch | 0.92 | 0.829 | -4.5 | 2.2 -> 13.0 |
| timestretch-batch | 1.08 | 0.843 | +2.0 | 2.2 -> 13.3 |
| signalsmith | 0.92 | 0.883 | -0.5 | 2.2 -> 7.2 |
| signalsmith | 1.08 | 0.900 | +0.5 | 2.2 -> 3.7 |
| rubberband-rt | 0.92 | 0.924 | -1.0 | 2.2 -> 3.2 |
| rubberband-rt | 1.08 | 0.927 | +1.0 | 2.2 -> 2.7 |
| rubberband-rt-r3 | 0.92 | 0.915 | -0.5 | 2.2 -> 5.4 |
| rubberband-rt-r3 | 1.08 | 0.927 | +0.5 | 2.2 -> 7.1 |
| rubberband-offline | 0.92 | 0.921 | -2.5 | 2.2 -> 2.9 |
| rubberband-offline | 1.08 | 0.929 | +2.0 | 2.2 -> 3.0 |
| rubberband-rt-smoothing | 0.92 | 0.916 | -1.5 | 2.2 -> 3.7 |
| rubberband-rt-smoothing | 1.08 | 0.916 | +1.5 | 2.2 -> 4.6 |
| rubberband-rt-mixed | 0.92 | 0.870 | -2.0 | 2.2 -> 14.5 |
| rubberband-rt-mixed | 1.08 | 0.859 | +1.0 | 2.2 -> 17.2 |

#### CPU

| engine | tempo | mono ms CPU per audio-s (chord / drums) | mono % core | STEREO ms CPU per audio-s (chord / drums) | STEREO % core per deck (worst) |
|---|---|---|---|---|---|
| wsola | 0.92 | 38.0 / 37.8 | 3.80 | 61.0 / 61.9 | 6.19 |
| wsola | 1.08 | 31.3 / 30.8 | 3.13 | 53.1 / 50.2 | 5.31 |
| timestretch-keylock | 0.92 | 16.9 / 16.7 | 1.69 | 20.8 / 19.7 | 2.08 |
| timestretch-keylock | 1.08 | 21.4 / 21.2 | 2.14 | 22.1 / 23.0 | 2.30 |
| timestretch-wide | 0.92 | 26.2 / 18.2 | 2.62 | 49.8 / 35.7 | 4.98 |
| timestretch-wide | 1.08 | 32.2 / 24.4 | 3.22 | 57.1 / 40.4 | 5.71 |
| timestretch-batch | 0.92 | 27.1 / 25.3 | 2.71 | 26.7 / 28.9 | 2.89 |
| timestretch-batch | 1.08 | 29.6 / 30.1 | 3.01 | 27.4 / 31.3 | 3.13 |
| signalsmith | 0.92 | 5.9 / 5.9 | 0.59 | 9.8 / 9.6 | 0.98 |
| signalsmith | 1.08 | 4.9 / 4.8 | 0.49 | 7.7 / 8.2 | 0.82 |
| rubberband-rt | 0.92 | 11.4 / 9.0 | 1.14 | 22.9 / 19.8 | 2.29 |
| rubberband-rt | 1.08 | 8.0 / 6.4 | 0.80 | 15.3 / 12.7 | 1.53 |
| rubberband-rt-r3 | 0.92 | 27.7 / 26.8 | 2.77 | 56.0 / 54.3 | 5.60 |
| rubberband-rt-r3 | 1.08 | 25.4 / 23.5 | 2.54 | 48.6 / 46.8 | 4.86 |
| rubberband-offline | 0.92 | 10.7 / 8.8 | 1.07 | 18.9 / 14.9 | 1.89 |
| rubberband-offline | 1.08 | 12.4 / 10.3 | 1.24 | 23.0 / 19.1 | 2.30 |
| rubberband-rt-smoothing | 0.92 | 14.2 / 12.3 | 1.42 | 30.2 / 26.7 | 3.02 |
| rubberband-rt-smoothing | 1.08 | 9.8 / 8.4 | 0.98 | 19.6 / 16.9 | 1.96 |
| rubberband-rt-mixed | 0.92 | 15.0 / 12.9 | 1.50 | 29.1 / 26.5 | 2.91 |
| rubberband-rt-mixed | 1.08 | 10.1 / 9.4 | 1.01 | 21.2 / 17.4 | 2.12 |


## What the numbers say

- **Pitch.** `wsola`, `timestretch-wide` and `signalsmith` are within ~1 cent on both estimators. `rubberband-rt-r3` is within 3.4 cents on the stricter chord peak estimator and exact on the sine. `timestretch-keylock` and `-batch` read 5.6 cents on the sine at 0.92 (fail < 5) and 12-27 cents on the strict chord-peak estimator, 2.6-5.0 cents on the centroid estimator.
- **Finding, Rubber Band defaults on steady synthetic tones.** Rubber Band 4.0.0 with default options (R2 engine, crisp transients) shifts a pure 440 Hz sine by 20-23 cents (a 434 Hz zero-crossing count confirms it is a real pitch change in the output, not a metric artifact). With `TRANSIENTS_SMOOTH`, `WINDOW_LONG` or `SMOOTHING_ON` the offline sine came out exact in a one-off option sweep (`stretch debug-rb`), so I believe the transient phase-reset logic fires on the perfectly steady tone, but **I did not root-cause it**, and I cannot rule out a build-flag issue in the vendored `rubberband-sys` single-file build (R3 on the same build is exact). Treat the R2 pitch numbers as "these defaults on perfectly steady sines", not as Rubber Band's behaviour on music. The `smoothing`/`mixed` variants were chosen only after seeing results, so their pitch numbers are tuned on this material and should not be over-trusted (and `rubberband-rt-mixed` costs transient quality: 14-17 ms rise).
- **Transients.** NCC alone barely separates engines (0.83-0.93). By attack rise time: Rubber Band R2 defaults/offline (2.7-3.2 ms), `wsola` (2.6 / 4.6 ms) and `rubberband-rt-smoothing` (3.7-4.6) are crisp; `signalsmith` 3.7-7.2; R3 5.4-7.1; every `timestretch` profile 12-15 ms (smeared; I did not supply its pre-analysis artifact for the real-time engine, which the crate says steers splices around onsets, so its real-time number may be pessimistic; the batch path does compute it and was no better). `wsola`'s +11/+12 ms best lag is a fixed output delay (about one hop), not smear.
- **CPU (stereo is the judged number).** Stereo per deck: signalsmith 0.8-1.0%, timestretch Keylock/batch 2.1-3.1%, Rubber Band R2 variants 1.5-3.0%, Rubber Band R3 4.9-5.6%, timestretch-wide 5.0-5.7% (4.98% at 0.92), wsola 5.3-6.2%. So wsola (5.3-6.2%) is over the 5% line, while timestretch-wide (4.98% at 0.92, 5.71% at 1.08) and R3 (4.86% at 1.08, 5.60% at 0.92) straddle it in stereo; only signalsmith and the cheap (but pitch- or transient-failing) variants are clearly under. Stereo cost roughly 1.6-2.2x mono for wsola, Rubber Band and timestretch-wide, and about 1.7x for signalsmith (not "doubled"), but barely changed `timestretch` Keylock/batch (cause not investigated). Each figure is the median of 5 wall-clock runs, +-20% plausible. Single-render throughput only, not callback worst case.

## User listening checklist

Nothing below was heard by me. Play the files in `spikes/stretch/out/` (regenerate with `cd spikes/stretch && cargo build --release && ./target/release/stretch-spike eval`, ~35 s). Files are 16-bit mono 44.1 kHz, peak <= 0.5. Originals: `chord_orig.wav`, `drums_orig.wav`. Stretched: `<material>_r<ratio>_<engine>.wav` for material in {`chord`, `drums`}, ratio in {`0.92`, `1.08`}, engine in {`wsola`, `timestretch-keylock`, `timestretch-wide`, `timestretch-batch`, `signalsmith`, `rubberband-rt`, `rubberband-rt-r3`, `rubberband-offline`, `rubberband-rt-smoothing`, `rubberband-rt-mixed`}, e.g. `drums_r1.08_wsola.wav`, `chord_r0.92_rubberband-rt-r3.wav`. 40 files plus 2 originals plus `results.md`.

Listen for, on `drums_*`: kick/snare/click attacks softened, pre-echo before hits, flams/doubled hits; on `chord_*`: warble/tremolo, phasiness ("underwater"), pitch sounding flat/sharp vs original, repeated-grain buzz. Suggested A/B order: `drums_r1.08` and `chord_r1.08` for `wsola` vs `signalsmith` vs `rubberband-rt-r3` vs `timestretch-wide`. **Important:** these are synthetic sines and drum synths; the real decision needs one or two real tracks run through `stretch <in.wav> <ratio> <out.wav> <engine>` (44.1 kHz mono or stereo WAV input; use your own music, none is in the repo).

## Caveats (honest)

- Synthetic mono material only; real polyphonic, vocal, and dense mixes are where WSOLA-class methods usually fall down and are untested. No stereo pitch/transient metrics, no formant, no variable-tempo (live nudge/ramps) test; real-time retargeting behaviour of each library was not exercised.
- The real-time claims are about throughput (compute per second of audio), not callback-deadline behaviour; no per-callback worst-case timing, no allocation check, no latency measurement beyond noting `wsola`'s ~11 ms offset.
- The adapters pad/truncate every output to exactly `round(n / ratio)` samples, so the harness length tests (`cargo test --release`) are weak for the length of a given engine; they do prove no engine panics or emits NaN at ratio 1.0 and 1.08.
- Engine configs were mostly defaults; only Rubber Band got an option sweep. Other engines may have untried settings (`wsola` hop/search, `timestretch` profiles with pre-analysis, Signalsmith presets).
- Peak memory not measured. Candidate maintenance: all four are young (0.x) crates with a single maintainer visible; I did not audit upkeep, issue trackers, or security beyond the licence field.
- Timing is wall-clock on a shared WSL2 VM.
- The decision on `deny.toml` (`GPL-2.0-or-later`) is left to Task 9. Signalsmith and `wsola`/`timestretch` need no licence change.
