//! Conformance of the meter against the signals in EBU Tech 3341 (loudness metering), Tech 3342
//! (loudness range) and the true-peak cases of Tech 3341.
//!
//! The official EBU test-set zip is not used: tech.ebu.ch refuses automated downloads (HTTP 403 bot
//! check), so every signal here is synthesised from the case descriptions in the two public PDFs, at
//! 48 kHz like the official files. Cases 6 (5.0 channels), 7 and 8 (authentic programme material)
//! and the Tech 3342 programme cases 5 and 6 need the official recordings and are not covered. The
//! true-peak cases 20 to 23 need the "lowpass and downsample" step of the official files, which the
//! PDF leaves unspecified; they use a windowed-sinc filter chosen here, so they are an approximation
//! of those files, not a replacement.

use super::*;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use std::time::Duration;

const FS: u32 = 48_000;

fn db(level: f64) -> f64 {
    10.0_f64.powf(level / 20.0)
}

/// A piece of a test signal. `Tone` is a 1000 Hz stereo sine, in phase, at that per-channel peak
/// level in dBFS; `Silence` is digital zero.
#[derive(Clone, Copy)]
enum Seg {
    Tone(f64, f64),
    Silence(f64),
}

fn render(rate: u32, segs: &[Seg]) -> Vec<f32> {
    let mut out = Vec::new();
    for seg in segs {
        match *seg {
            Seg::Tone(secs, dbfs) => {
                let amp = db(dbfs);
                let n = (secs * f64::from(rate)).round() as usize;
                for i in 0..n {
                    let phase = 2.0 * std::f64::consts::PI * 1000.0 * i as f64 / f64::from(rate);
                    let v = (amp * phase.sin()) as f32;
                    out.push(v);
                    out.push(v);
                }
            }
            Seg::Silence(secs) => {
                let n = (secs * f64::from(rate)).round() as usize;
                out.resize(out.len() + n * 2, 0.0);
            }
        }
    }
    out
}

/// Measures a whole signal on a fresh meter.
fn measure(rate: u32, signal: &[f32]) -> LoudnessReading {
    let mut a = LoudnessAnalyser::new(rate).expect("meter");
    a.add_interleaved(signal);
    a.reading()
}

fn assert_close(what: &str, got: f32, want: f64, tol: f64) {
    assert!(
        (f64::from(got) - want).abs() <= tol,
        "{what}: got {got}, want {want} +/- {tol}"
    );
}

// ---- Tech 3341 table 1, cases 1 to 5: M, S and I of steady and gated tones ----

#[test]
fn tech3341_case1_and_2_steady_tone() {
    for (case, level, want) in [(1, -23.0, -23.0), (2, -33.0, -33.0)] {
        let r = measure(FS, &render(FS, &[Seg::Tone(20.0, level)]));
        assert_close(&format!("case {case} M"), r.momentary_lufs, want, 0.1);
        assert_close(&format!("case {case} S"), r.short_term_lufs, want, 0.1);
        assert_close(&format!("case {case} I"), r.integrated_lufs, want, 0.1);
    }
}

#[test]
fn tech3341_case3_relative_gate() {
    let sig = render(
        FS,
        &[
            Seg::Tone(10.0, -36.0),
            Seg::Tone(60.0, -23.0),
            Seg::Tone(10.0, -36.0),
        ],
    );
    assert_close("case 3 I", measure(FS, &sig).integrated_lufs, -23.0, 0.1);
}

#[test]
fn tech3341_case4_absolute_gate() {
    let sig = render(
        FS,
        &[
            Seg::Tone(10.0, -72.0),
            Seg::Tone(10.0, -36.0),
            Seg::Tone(60.0, -23.0),
            Seg::Tone(10.0, -36.0),
            Seg::Tone(10.0, -72.0),
        ],
    );
    assert_close("case 4 I", measure(FS, &sig).integrated_lufs, -23.0, 0.1);
}

#[test]
fn tech3341_case5_gating_of_a_louder_middle() {
    let sig = render(
        FS,
        &[
            Seg::Tone(20.0, -26.0),
            Seg::Tone(20.1, -20.0),
            Seg::Tone(20.0, -26.0),
        ],
    );
    assert_close("case 5 I", measure(FS, &sig).integrated_lufs, -23.0, 0.1);
}

// ---- Tech 3341 table 1, cases 9 to 14: time response of S and M ----

/// Feeds `signal` in `block`-frame steps and calls `each(seconds, reading)` after every step.
fn walk(rate: u32, signal: &[f32], block: usize, mut each: impl FnMut(f64, &LoudnessReading)) {
    let mut a = LoudnessAnalyser::new(rate).expect("meter");
    let mut frames = 0_usize;
    for chunk in signal.chunks(block * 2) {
        a.add_interleaved(chunk);
        frames += chunk.len() / 2;
        each(frames as f64 / f64::from(rate), &a.reading());
    }
}

#[test]
fn tech3341_case9_short_term_of_alternating_levels() {
    let mut segs = Vec::new();
    for _ in 0..5 {
        segs.push(Seg::Tone(1.34, -20.0));
        segs.push(Seg::Tone(1.66, -30.0));
    }
    let mut checked = 0;
    walk(FS, &render(FS, &segs), 4_800, |t, r| {
        // "Constant after 3 s": each 3 s window holds one full 1.34 + 1.66 s cycle.
        if t >= 3.05 {
            assert_close("case 9 S", r.short_term_lufs, -23.0, 0.1);
            checked += 1;
        }
    });
    assert!(checked > 100);
}

#[test]
fn tech3341_case12_momentary_of_alternating_levels() {
    let mut segs = Vec::new();
    for _ in 0..25 {
        segs.push(Seg::Tone(0.18, -20.0));
        segs.push(Seg::Tone(0.22, -30.0));
    }
    let mut checked = 0;
    walk(FS, &render(FS, &segs), 480, |t, r| {
        if t >= 1.0 {
            assert_close("case 12 M", r.momentary_lufs, -23.0, 0.1);
            checked += 1;
        }
    });
    assert!(checked > 100);
}

#[test]
fn tech3341_case10_file_based_short_term_maximum() {
    for i in 0..20 {
        let sig = render(
            FS,
            &[
                Seg::Silence(f64::from(i) * 0.15),
                Seg::Tone(3.0, -23.0),
                Seg::Silence(1.0),
            ],
        );
        let mut max = f32::NEG_INFINITY;
        walk(FS, &sig, 480, |_, r| max = max.max(r.short_term_lufs));
        assert_close(&format!("case 10 segment {i} max S"), max, -23.0, 0.1);
    }
}

#[test]
fn tech3341_case13_file_based_momentary_maximum() {
    for i in 0..20 {
        let sig = render(
            FS,
            &[
                Seg::Silence(f64::from(i) * 0.02),
                Seg::Tone(0.4, -23.0),
                Seg::Silence(1.0),
            ],
        );
        let mut max = f32::NEG_INFINITY;
        walk(FS, &sig, 48, |_, r| max = max.max(r.momentary_lufs));
        assert_close(&format!("case 13 segment {i} max M"), max, -23.0, 0.1);
    }
}

#[test]
fn tech3341_case11_live_short_term_maxima() {
    let mut segs = Vec::new();
    for i in 0..20 {
        segs.push(Seg::Silence(f64::from(i) * 0.15));
        segs.push(Seg::Tone(3.0, -38.0 + f64::from(i)));
        segs.push(Seg::Silence(3.0 - f64::from(i) * 0.15));
    }
    let mut maxima = [f32::NEG_INFINITY; 20];
    // Slot i: i * 0.15 s silence, 3 s tone, (3 - i * 0.15) s silence, so every slot lasts 6 s.
    let slot = 6.0;
    walk(FS, &render(FS, &segs), 480, |t, r| {
        let seg = (((t - 1e-9) / slot).floor() as usize).min(19);
        maxima[seg] = maxima[seg].max(r.short_term_lufs);
    });
    for (i, m) in maxima.iter().enumerate() {
        assert_close(&format!("case 11 tone {i}"), *m, -38.0 + i as f64, 0.1);
    }
}

#[test]
fn tech3341_case14_live_momentary_maxima() {
    // Slot i: i * 20 ms silence, 400 ms tone at -38 + i dBFS, then (400 - i * 20) ms silence, so
    // every slot lasts 0.8 s.
    let mut segs = Vec::new();
    for i in 0..20 {
        segs.push(Seg::Silence(f64::from(i) * 0.02));
        segs.push(Seg::Tone(0.4, -38.0 + f64::from(i)));
        segs.push(Seg::Silence(0.4 - f64::from(i) * 0.02));
    }
    let mut maxima = [f32::NEG_INFINITY; 20];
    walk(FS, &render(FS, &segs), 48, |t, r| {
        let seg = (((t - 1e-9) / 0.8).floor() as usize).min(19);
        maxima[seg] = maxima[seg].max(r.momentary_lufs);
    });
    for (i, m) in maxima.iter().enumerate() {
        assert_close(&format!("case 14 tone {i}"), *m, -38.0 + i as f64, 0.1);
    }
}

// ---- Tech 3342 table 1, cases 1 to 4: loudness range ----

#[test]
fn tech3342_loudness_range_cases() {
    let cases: [(&str, &[f64], f64); 4] = [
        ("1", &[-20.0, -30.0], 10.0),
        ("2", &[-20.0, -15.0], 5.0),
        ("3", &[-40.0, -20.0], 20.0),
        ("4", &[-50.0, -35.0, -20.0, -35.0, -50.0], 15.0),
    ];
    for (name, levels, want) in cases {
        let segs: Vec<Seg> = levels.iter().map(|l| Seg::Tone(20.0, *l)).collect();
        let sig = render(FS, &segs);
        assert_close(
            &format!("3342 case {name} LRA"),
            measure(FS, &sig).loudness_range_lu,
            want,
            1.0,
        );
        // "The expected response is unchanged if the test signal is repeated."
        let twice = [sig.as_slice(), sig.as_slice()].concat();
        assert_close(
            &format!("3342 case {name} LRA repeated"),
            measure(FS, &twice).loudness_range_lu,
            want,
            1.0,
        );
    }
}

// ---- Tech 3341 table 1, cases 15 to 23: true peak ----

/// Stereo sine at `freq`, peak `amp` (linear, relative to full scale), start phase in degrees.
fn sine(freq: f64, amp: f64, phase_deg: f64, n: usize) -> Vec<f32> {
    let w = 2.0 * std::f64::consts::PI * freq / f64::from(FS);
    let fade = (0.010 * f64::from(FS)) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let env = if i < fade {
            i as f64 / fade as f64
        } else if i >= n - fade {
            (n - 1 - i) as f64 / fade as f64
        } else {
            1.0
        };
        let v = (env * amp * (w * i as f64 + phase_deg.to_radians()).sin()) as f32;
        out.push(v);
        out.push(v);
    }
    out
}

fn assert_true_peak(what: &str, r: &LoudnessReading, want_dbtp: f64) {
    for (ch, got) in r.true_peak_dbtp.iter().enumerate() {
        let got = f64::from(*got);
        // Tolerance +0.2 / -0.4 dB.
        assert!(
            got <= want_dbtp + 0.2 && got >= want_dbtp - 0.4,
            "{what} ch{ch}: {got} dBTP, want {want_dbtp} +0.2/-0.4"
        );
    }
}

#[test]
fn tech3341_true_peak_cases_15_to_19() {
    let n = FS as usize; // duration does not matter
    let fs = f64::from(FS);
    let cases = [
        ("15", fs / 4.0, 0.5, 0.0, -6.0),
        ("16", fs / 4.0, 0.5, 45.0, -6.0),
        ("17", fs / 6.0, 0.5, 60.0, -6.0),
        ("18", fs / 8.0, 0.5, 67.5, -6.0),
        ("19", fs / 4.0, 1.41, 45.0, 3.0),
    ];
    for (name, freq, amp, phase, want) in cases {
        let r = measure(FS, &sine(freq, amp, phase, n));
        assert_true_peak(&format!("case {name}"), &r, want);
    }
}

/// Cases 20 to 23: an fs/6 carrier at half scale with one period of an fs/4 sine at full scale in
/// the middle, synthesised at 4 fs, lowpassed and decimated by 4 with a sample offset.
fn single_period_signal(offset: usize) -> Vec<f32> {
    let hi = 4 * FS as usize;
    let carrier = |i: usize| 0.5 * (2.0 * std::f64::consts::PI * i as f64 / 24.0).sin();
    let total = hi; // one second
    let start = (total / 2) / 24 * 24; // a zero crossing of the carrier
    let mut x = vec![0.0_f64; total];
    for (i, v) in x.iter_mut().enumerate() {
        *v = carrier(i);
    }
    for k in 0..16 {
        // fs/4 at 4 fs is 16 samples per period; it leaves the carrier at phase zero too.
        x[start + k] = (2.0 * std::f64::consts::PI * k as f64 / 16.0).sin();
    }
    // Windowed-sinc lowpass, cutoff 0.45 fs (0.1125 of the 4 fs rate), 255 taps, Blackman.
    let taps = 255_usize;
    let fc = 0.1125_f64;
    let mid = (taps / 2) as f64;
    let h: Vec<f64> = (0..taps)
        .map(|k| {
            let m = k as f64 - mid;
            let sinc = if m == 0.0 {
                2.0 * fc
            } else {
                (2.0 * std::f64::consts::PI * fc * m).sin() / (std::f64::consts::PI * m)
            };
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / (taps - 1) as f64).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * k as f64 / (taps - 1) as f64).cos();
            sinc * w
        })
        .collect();
    let filtered: Vec<f64> = (0..total)
        .map(|i| {
            h.iter()
                .enumerate()
                .map(|(k, hk)| {
                    let j = i as isize + k as isize - taps as isize / 2;
                    if j >= 0 && (j as usize) < total {
                        hk * x[j as usize]
                    } else {
                        0.0
                    }
                })
                .sum()
        })
        .collect();
    let fade = (0.010 * f64::from(FS)) as usize;
    let n = (total - offset) / 4;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let env = (i.min(n - 1 - i) as f64 / fade as f64).min(1.0);
        let v = (env * filtered[i * 4 + offset]) as f32;
        out.push(v);
        out.push(v);
    }
    out
}

#[test]
fn tech3341_true_peak_cases_20_to_23_approximation() {
    for (case, offset) in [(20, 0), (21, 1), (22, 2), (23, 3)] {
        let r = measure(FS, &single_period_signal(offset));
        let top = f64::from(r.true_peak_max_dbtp());
        // The filter is ours, so a loose reading of the stated 0.0 +0.2/-0.4 dBTP: the meter must
        // see a peak near full scale, whatever the decimation phase.
        assert!(
            (-1.0..=0.2).contains(&top),
            "case {case}: {top} dBTP is far from the 0.0 dBTP the case describes"
        );
    }
}

// ---- Other sample rates ----

#[test]
fn a_997_hz_style_tone_reads_the_same_at_every_common_rate() {
    for rate in [44_100_u32, 48_000, 88_200, 96_000] {
        let signal = render(rate, &[Seg::Tone(20.0, -20.0)]);
        let r = measure(rate, &signal);
        assert_close(&format!("{rate} Hz I"), r.integrated_lufs, -20.0, 0.1);
        assert_close(&format!("{rate} Hz M"), r.momentary_lufs, -20.0, 0.1);
    }
}

#[test]
fn silence_reads_as_nothing_not_as_a_number() {
    let r = measure(FS, &render(FS, &[Seg::Silence(5.0)]));
    assert_eq!(r.momentary_lufs, f32::NEG_INFINITY);
    assert_eq!(r.short_term_lufs, f32::NEG_INFINITY);
    assert_eq!(r.integrated_lufs, f32::NEG_INFINITY);
    assert_eq!(r.true_peak_dbtp, [f32::NEG_INFINITY; 2]);
    assert_eq!(r.loudness_range_lu, 0.0);
}

#[test]
fn rejects_a_rate_the_crate_cannot_meter() {
    assert!(LoudnessAnalyser::new(1).is_err());
}

#[test]
fn reset_forgets_integrated_and_peak() {
    let mut a = LoudnessAnalyser::new(FS).expect("meter");
    a.add_interleaved(&render(FS, &[Seg::Tone(5.0, -10.0)]));
    assert!(a.reading().integrated_lufs > -15.0);
    a.reset();
    let r = a.reading();
    assert_eq!(r.integrated_lufs, f32::NEG_INFINITY);
    assert_eq!(r.true_peak_dbtp, [f32::NEG_INFINITY; 2]);
}

// ---- The threaded path: tap, ring, worker, atomics ----

/// Pushes `signal` through the tap in audio-sized blocks, waiting for the worker whenever the ring
/// could fill, as a real-time callback paced by the device would be.
fn play(tap: &mut LoudnessTap, control: &LoudnessControl, signal: &[f32]) {
    const BLOCK: usize = 256;
    let mut sent = control.reading().frames_measured;
    let (mut l, mut r) = (vec![0.0_f32; BLOCK], vec![0.0_f32; BLOCK]);
    for chunk in signal.chunks(BLOCK * 2) {
        let frames = chunk.len() / 2;
        for (i, f) in chunk.chunks_exact(2).enumerate() {
            l[i] = f[0];
            r[i] = f[1];
        }
        // Stay under half the ring ahead of the worker.
        let ahead_limit = (RING_SAMPLES / 4) as u64;
        assert!(
            wait_until(Duration::from_secs(10), Duration::from_micros(200), || {
                sent.saturating_sub(control.reading().frames_measured) < ahead_limit
            }),
            "worker stalled"
        );
        tap.process(&l[..frames], &r[..frames]);
        sent += frames as u64;
    }
    assert!(
        wait_until(Duration::from_secs(10), DEFAULT_POLL, || {
            control.reading().frames_measured >= sent
        }),
        "worker did not finish"
    );
}

#[test]
fn worker_publishes_the_same_values_as_the_analyser() {
    let (mut tap, control) = Loudness::create();
    tap.prepare(f64::from(FS));
    control.enable().expect("worker starts");
    let signal = render(FS, &[Seg::Tone(10.0, -23.0)]);
    play(&mut tap, &control, &signal);
    let r = control.reading();
    assert_close("M", r.momentary_lufs, -23.0, 0.1);
    assert_close("S", r.short_term_lufs, -23.0, 0.1);
    assert_close("I", r.integrated_lufs, -23.0, 0.1);
    assert_close("true peak L", r.true_peak_dbtp[0], -23.0, 0.3);
    assert_eq!(r.dropped_frames, 0);
    assert_eq!(r.frames_measured, 10 * u64::from(FS));
}

#[test]
fn reset_through_the_control_clears_the_published_values() {
    let (mut tap, control) = Loudness::create();
    tap.prepare(f64::from(FS));
    control.enable().expect("worker starts");
    play(&mut tap, &control, &render(FS, &[Seg::Tone(3.0, -20.0)]));
    assert!(control.reading().integrated_lufs.is_finite());
    control.reset();
    assert!(
        wait_until(Duration::from_secs(5), DEFAULT_POLL, || {
            control.reading().integrated_lufs == f32::NEG_INFINITY
        }),
        "reset was not applied"
    );
}

#[test]
fn a_disabled_meter_does_not_touch_the_ring_or_start_a_thread() {
    let (mut tap, control) = Loudness::create();
    tap.prepare(f64::from(FS));
    tap.process(&[0.5; 256], &[0.5; 256]);
    assert!(!control.is_enabled());
    assert_eq!(tap.tx.slots(), RING_SAMPLES);
    assert_eq!(control.reading(), LoudnessReading::SILENT);
}

#[test]
fn a_full_ring_counts_dropped_frames_and_never_blocks() {
    let (mut tap, control) = Loudness::create();
    tap.prepare(f64::from(FS));
    // Enabled, but no worker running (enable() would start one): nothing drains the ring.
    tap.shared.enabled.store(true, Relaxed);
    let block = [0.25_f32; 1024];
    let blocks = 2 * RING_SAMPLES / 2 / block.len();
    for _ in 0..blocks {
        tap.process(&block, &block);
    }
    let kept = (RING_SAMPLES / 2) as u64;
    let sent = (blocks * block.len()) as u64;
    assert_eq!(control.reading().dropped_frames, sent - kept);
}

#[test]
fn dropping_the_control_stops_the_worker() {
    let (mut tap, control) = Loudness::create();
    tap.prepare(f64::from(FS));
    control.enable().expect("worker starts");
    let started = std::time::Instant::now();
    drop(control);
    assert!(started.elapsed() < Duration::from_secs(2));
    // The tap outlives the control without trouble.
    tap.process(&[0.1; 64], &[0.1; 64]);
}
