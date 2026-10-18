# Spike C: time-stretch (keylock) quality

THROWAWAY spike code in `spikes/stretch/` (own `[workspace]`; not built by the root workspace or Rust CI; `Cargo.lock` is committed, `target/` and `out/` are gitignored). Date: 2026-10-18. Everything is **offline**: WAV files in, WAV files out. No audio device is opened and nothing is played by the harness, tests, or the agent run. The generated WAVs in `spikes/stretch/out/` are for the user to listen to.

## Verdict

**Provisional, not final: the numbers cannot close this spike; ears are required.**

- No pure-Rust candidate is a clean pass on every measured axis. `wsola` (pure Rust, MIT) passes every numeric bar here (pitch < 1 cent, transients the closest to Rubber Band, ~3.6% of a core mono) but it is the simplest algorithm, is brand new, and my test material is far easier than real dense music, so its numbers say little about how it sounds on a full mix. The `timestretch` crate (pure Rust, MIT, the most DJ-targeted) keeps pitch only in its `WideKeylock` profile and smears transients about 5x (12-15 ms attack vs 2.2 ms), failing the objective transient comparison.
- Best measured all-round: **Signalsmith Stretch via `signalsmith-stretch` (MIT, C++ compiled with `cc`, so FFI, not pure Rust)**: pitch < 1 cent, mid transients, and by far the lowest CPU (~0.6% of a core mono). Rubber Band R3 (finer) is also clean on pitch with the highest CPU (~2.8% mono, so ~5.5% stereo by simple doubling, unmeasured, right at the 5% line).
- Recommendation for Task 9, to be confirmed by the user's A/B listen: if `wsola` sounds acceptable on real music, take it (pure Rust, ADR preference). If not, take **Signalsmith via a thin FFI crate** before Rubber Band: it is MIT, so it does **not** need the GPL-2.0-or-later entry in `deny.toml`, and it measured best on CPU. Choosing Rubber Band FFI instead means enabling the `GPL-2.0-or-later` line in `deny.toml` (a Task 9 decision, not made here); use its R3 engine (`ENGINE_FINER`), not the defaults, see the finding below.
- Design bar status: pitch error < 5 cents: wsola, timestretch-wide, signalsmith, rubberband-rt-r3 pass; timestretch-keylock/batch fail at 0.92 (5.6 cents on a pure sine). One deck < 5% of a core: all pass in mono; see stereo caveat.

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

Harness: `spikes/stretch/` (`cargo build --release`; run from that directory). Commands: `stretch <in.wav> <ratio> <out.wav> [engine]`, `stretch gen`, `stretch eval`, `stretch engines`. `ratio` is the DJ tempo factor: `1.08` = 8% faster, output length = input / ratio, pitch preserved. `hound` for WAV (16-bit mono out). Engines (mono, 44.1 kHz, single thread):

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

## Results

All numbers below are output of `cargo build --release && ./target/release/stretch-spike eval` (also saved to `spikes/stretch/out/results.md`, not committed). Design bar: pitch < 5 cents; one deck < 5% of a core.

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

| engine | tempo | ms CPU per audio-second (chord / drums) | % of one core (worst, mono) |
|---|---|---|---|
| wsola | 0.92 | 35.9 / 35.5 | 3.59 |
| wsola | 1.08 | 29.6 / 30.2 | 3.02 |
| timestretch-keylock | 0.92 | 16.9 / 17.0 | 1.70 |
| timestretch-keylock | 1.08 | 20.8 / 21.1 | 2.11 |
| timestretch-wide | 0.92 | 24.8 / 18.3 | 2.48 |
| timestretch-wide | 1.08 | 29.8 / 23.2 | 2.98 |
| timestretch-batch | 0.92 | 25.8 / 24.6 | 2.58 |
| timestretch-batch | 1.08 | 29.7 / 28.8 | 2.97 |
| signalsmith | 0.92 | 5.7 / 5.6 | 0.57 |
| signalsmith | 1.08 | 4.9 / 5.0 | 0.50 |
| rubberband-rt | 0.92 | 12.1 / 9.4 | 1.21 |
| rubberband-rt | 1.08 | 8.0 / 6.7 | 0.80 |
| rubberband-rt-r3 | 0.92 | 27.7 / 26.9 | 2.77 |
| rubberband-rt-r3 | 1.08 | 25.2 / 24.2 | 2.52 |
| rubberband-offline | 0.92 | 10.2 / 8.3 | 1.02 |
| rubberband-offline | 1.08 | 12.8 / 10.7 | 1.28 |
| rubberband-rt-smoothing | 0.92 | 15.1 / 12.5 | 1.51 |
| rubberband-rt-smoothing | 1.08 | 9.9 / 8.8 | 0.99 |
| rubberband-rt-mixed | 0.92 | 14.9 / 12.8 | 1.49 |
| rubberband-rt-mixed | 1.08 | 10.2 / 8.9 | 1.02 |


## What the numbers say

- **Pitch.** `wsola`, `timestretch-wide` and `signalsmith` are within ~1 cent on both estimators. `rubberband-rt-r3` is within 3.4 cents on the stricter chord peak estimator and exact on the sine. `timestretch-keylock` and `-batch` read 5.6 cents on the sine at 0.92 (fail < 5) and 12-27 cents on the strict chord-peak estimator, 2.6-5.0 cents on the centroid estimator.
- **Finding, Rubber Band defaults on steady synthetic tones.** Rubber Band 4.0.0 with default options (R2 engine, crisp transients) shifts a pure 440 Hz sine by 20-23 cents (a 434 Hz zero-crossing count confirms it is a real pitch change in the output, not a metric artifact). With `TRANSIENTS_SMOOTH`, `WINDOW_LONG` or `SMOOTHING_ON` the offline sine came out exact in a one-off option sweep (`stretch debug-rb`), so I believe the transient phase-reset logic fires on the perfectly steady tone, but **I did not root-cause it**, and I cannot rule out a build-flag issue in the vendored `rubberband-sys` single-file build (R3 on the same build is exact). Treat the R2 pitch numbers as "these defaults on perfectly steady sines", not as Rubber Band's behaviour on music. The `smoothing`/`mixed` variants were chosen only after seeing results, so their pitch numbers are tuned on this material and should not be over-trusted (and `rubberband-rt-mixed` costs transient quality: 14-17 ms rise).
- **Transients.** NCC alone barely separates engines (0.83-0.93). By attack rise time: Rubber Band R2 defaults/offline (2.7-3.2 ms), `wsola` (2.6 / 4.6 ms) and `rubberband-rt-smoothing` (3.7-4.6) are crisp; `signalsmith` 3.7-7.2; R3 5.4-7.1; every `timestretch` profile 12-15 ms (smeared; I did not supply its pre-analysis artifact for the real-time engine, which the crate says steers splices around onsets, so its real-time number may be pessimistic; the batch path does compute it and was no better). `wsola`'s +11/+12 ms best lag is a fixed output delay (about one hop), not smear.
- **CPU.** Everything is far under 5% of a core for mono: signalsmith 0.5-0.7%, rubberband R2 0.8-1.5%, timestretch 1.7-3.1%, wsola 3.0-3.6%, rubberband R3 2.5-2.8%. A real deck is stereo; I measured mono only, so doubling gives wsola ~7% and R3 ~5.5%, over the bar, signalsmith ~1.2%. (Stereo cost for each library may not be exactly 2x; not measured.) The `wsola` crate is an unoptimised brute-force similarity search and could likely be sped up, but that is untested.

## User listening checklist

Nothing below was heard by me. Play the files in `spikes/stretch/out/` (regenerate with `cd spikes/stretch && cargo build --release && ./target/release/stretch-spike eval`, ~35 s). Files are 16-bit mono 44.1 kHz, peak <= 0.5. Originals: `chord_orig.wav`, `drums_orig.wav`. Stretched: `<material>_r<ratio>_<engine>.wav` for material in {`chord`, `drums`}, ratio in {`0.92`, `1.08`}, engine in {`wsola`, `timestretch-keylock`, `timestretch-wide`, `timestretch-batch`, `signalsmith`, `rubberband-rt`, `rubberband-rt-r3`, `rubberband-offline`, `rubberband-rt-smoothing`, `rubberband-rt-mixed`}, e.g. `drums_r1.08_wsola.wav`, `chord_r0.92_rubberband-rt-r3.wav`. 40 files plus 2 originals plus `results.md`.

Listen for, on `drums_*`: kick/snare/click attacks softened, pre-echo before hits, flams/doubled hits; on `chord_*`: warble/tremolo, phasiness ("underwater"), pitch sounding flat/sharp vs original, repeated-grain buzz. Suggested A/B order: `drums_r1.08` and `chord_r1.08` for `wsola` vs `signalsmith` vs `rubberband-rt-r3` vs `timestretch-wide`. **Important:** these are synthetic sines and drum synths; the real decision needs one or two real tracks run through `stretch <in.wav> <ratio> <out.wav> <engine>` (mono 44.1 kHz WAV input; use your own music, none is in the repo).

## Caveats (honest)

- Synthetic mono material only; real polyphonic, vocal, and dense mixes are where WSOLA-class methods usually fall down and are untested. No stereo, no formant, no variable-tempo (live nudge/ramps) test; real-time retargeting behaviour of each library was not exercised.
- The real-time claims are about throughput (compute per second of audio), not callback-deadline behaviour; no per-callback worst-case timing, no allocation check, no latency measurement beyond noting `wsola`'s ~11 ms offset.
- The adapters pad/truncate every output to exactly `round(n / ratio)` samples, so the harness length tests (`cargo test --release`) are weak for the length of a given engine; they do prove no engine panics or emits NaN at ratio 1.0 and 1.08.
- Engine configs were mostly defaults; only Rubber Band got an option sweep. Other engines may have untried settings (`wsola` hop/search, `timestretch` profiles with pre-analysis, Signalsmith presets).
- Peak memory not measured. Candidate maintenance: all four are young (0.x) crates with a single maintainer visible; I did not audit upkeep, issue trackers, or security beyond the licence field.
- Timing is wall-clock on a shared WSL2 VM.
- The decision on `deny.toml` (`GPL-2.0-or-later`) is left to Task 9. Signalsmith and `wsola`/`timestretch` need no licence change.
