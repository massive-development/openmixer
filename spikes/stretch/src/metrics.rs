//! THROWAWAY SPIKE: offline metrics + the `eval` driver.
//!
//! Methods (all stated in docs/spikes/stretch.md):
//! * pitch error: Hann-windowed 65536-pt FFT of the centre of the signal, magnitude peak near each
//!   known chord fundamental with parabolic interpolation on log-magnitude; error in cents of the
//!   stretched output peak vs the same measurement on the unstretched input (cancels bin bias).
//!   Reported as the max |cents| over the three fundamentals.
//! * transient smear: (a) normalised cross-correlation of onset envelopes (stretched output vs the
//!   original's envelope time-warped by 1/tempo), max over +-20 ms lag, after 5 ms smoothing;
//!   (b) mean 10%-to-peak rise time of the RMS envelope at each ideal onset, output minus original.
//! * CPU: wall-clock `Instant` over several runs of the whole 8 s mono render (single thread),
//!   median; reported as ms of compute per second of audio and as % of one core
//!   (audio-second = 1 s of INPUT audio consumed at that tempo; real-time needs <100%).
use crate::{engines, synth, write_wav_mono, SR};
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

// ---------- FFT ----------
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two());
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for s in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (s + k, s + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

const NFFT: usize = 65536;

/// Frequency (Hz) of the spectral peak within +-4% of `near_hz`, centre of `x`.
pub fn peak_hz(x: &[f32], near_hz: f64) -> f64 {
    assert!(x.len() >= NFFT, "need >= {NFFT} samples");
    let start = (x.len() - NFFT) / 2;
    let mut re: Vec<f64> = (0..NFFT)
        .map(|i| x[start + i] as f64 * (0.5 - 0.5 * (2.0 * PI * i as f64 / NFFT as f64).cos()))
        .collect();
    let mut im = vec![0.0; NFFT];
    fft(&mut re, &mut im);
    let mag: Vec<f64> = (0..NFFT / 2).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt() + 1e-30).collect();
    let bin = |hz: f64| hz * NFFT as f64 / SR as f64;
    let (lo, hi) = (bin(near_hz * 0.96) as usize, bin(near_hz * 1.04) as usize + 1);
    let k = (lo..hi).max_by(|&a, &b| mag[a].partial_cmp(&mag[b]).unwrap()).unwrap();
    let (a, b, c) = (mag[k - 1].ln(), mag[k].ln(), mag[k + 1].ln());
    let d = 0.5 * (a - c) / (a - 2.0 * b + c);
    (k as f64 + d) * SR as f64 / NFFT as f64
}

/// Power-weighted mean frequency (Hz) within +-3% of `near_hz`, centre of `x`. Robust to the
/// amplitude-modulation sidebands a phase vocoder can put around a steady partial, which move the
/// strict magnitude peak without moving the perceived pitch.
pub fn centroid_hz(x: &[f32], near_hz: f64) -> f64 {
    assert!(x.len() >= NFFT, "need >= {NFFT} samples");
    let start = (x.len() - NFFT) / 2;
    let mut re: Vec<f64> = (0..NFFT)
        .map(|i| x[start + i] as f64 * (0.5 - 0.5 * (2.0 * PI * i as f64 / NFFT as f64).cos()))
        .collect();
    let mut im = vec![0.0; NFFT];
    fft(&mut re, &mut im);
    let bin = |hz: f64| hz * NFFT as f64 / SR as f64;
    let (lo, hi) = (bin(near_hz * 0.97) as usize, bin(near_hz * 1.03) as usize + 1);
    let (mut num, mut den) = (0.0, 0.0);
    for k in lo..hi {
        let p = re[k] * re[k] + im[k] * im[k];
        num += p * k as f64;
        den += p;
    }
    num / den * SR as f64 / NFFT as f64
}

pub fn cents(f: f64, reference: f64) -> f64 {
    1200.0 * (f / reference).log2()
}

/// Max |cents| error across `comps` using the centroid estimator, output vs unstretched input.
pub fn pitch_error_centroid(orig: &[f32], out: &[f32], comps: &[f64]) -> f64 {
    comps
        .iter()
        .map(|&f| cents(centroid_hz(out, f), centroid_hz(orig, f)).abs())
        .fold(0.0, f64::max)
}

/// Max |cents| error across `comps` using the strict FFT-peak estimator, output vs unstretched input.
pub fn pitch_error_cents(orig: &[f32], out: &[f32], comps: &[f64]) -> f64 {
    comps
        .iter()
        .map(|&f| cents(peak_hz(out, f), peak_hz(orig, f)).abs())
        .fold(0.0, f64::max)
}

// ---------- onset / transient ----------
const HOP: usize = 22; // 0.5 ms
const WIN: usize = 66; // 1.5 ms

/// RMS envelope, hop 0.5 ms.
pub fn rms_env(x: &[f32]) -> Vec<f64> {
    (0..x.len().saturating_sub(WIN) / HOP)
        .map(|k| {
            let s = &x[k * HOP..k * HOP + WIN];
            (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / WIN as f64).sqrt()
        })
        .collect()
}

/// Onset-strength envelope: half-wave-rectified difference of sqrt-compressed RMS, 5 ms box smoothed.
pub fn onset_env(env: &[f64]) -> Vec<f64> {
    let c: Vec<f64> = env.iter().map(|v| v.sqrt()).collect();
    let d: Vec<f64> = (0..c.len()).map(|i| if i == 0 { 0.0 } else { (c[i] - c[i - 1]).max(0.0) }).collect();
    let w = 10;
    (0..d.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(w / 2), (i + w / 2 + 1).min(d.len()));
            d[a..b].iter().sum::<f64>() / (b - a) as f64
        })
        .collect()
}

/// Resample an envelope so that out[k] = env(k * tempo) (i.e. time-warp the original by 1/tempo).
pub fn warp(env: &[f64], tempo: f64) -> Vec<f64> {
    let n = (env.len() as f64 / tempo) as usize;
    (0..n)
        .map(|k| {
            let p = k as f64 * tempo;
            let i = p as usize;
            let f = p - i as f64;
            if i + 1 < env.len() { env[i] * (1.0 - f) + env[i + 1] * f } else { env[env.len() - 1] }
        })
        .collect()
}

/// Max normalised cross-correlation over lags +-max_lag (frames). Returns (ncc, best_lag_frames).
pub fn ncc(a: &[f64], b: &[f64], max_lag: i64) -> (f64, i64) {
    let n = a.len().min(b.len()) as i64;
    let mut best = (-2.0, 0);
    for lag in -max_lag..=max_lag {
        let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let j = i + lag;
            if j < 0 || j >= n {
                continue;
            }
            let (x, y) = (a[i as usize], b[j as usize]);
            sab += x * y;
            saa += x * x;
            sbb += y * y;
        }
        let r = sab / (saa.sqrt() * sbb.sqrt() + 1e-30);
        if r > best.0 {
            best = (r, lag);
        }
    }
    best
}

/// Onset positions (env frames): local maxima of onset env above 30% of max, >= 60 ms apart.
pub fn onsets(oenv: &[f64]) -> Vec<usize> {
    let mx = oenv.iter().cloned().fold(0.0, f64::max);
    let mut v: Vec<usize> = vec![];
    let gap = 120; // frames of 0.5 ms
    for i in 1..oenv.len().saturating_sub(1) {
        if oenv[i] > 0.3 * mx && oenv[i] >= oenv[i - 1] && oenv[i] > oenv[i + 1] {
            if let Some(&l) = v.last() {
                if i - l < gap {
                    if oenv[i] > oenv[l] {
                        *v.last_mut().unwrap() = i;
                    }
                    continue;
                }
            }
            v.push(i);
        }
    }
    v
}

/// Rise time in ms: from the last frame before the local peak (within +-30 ms of `centre`) where the
/// RMS envelope is below 10% of that peak, to the peak.
pub fn rise_ms(env: &[f64], centre: usize) -> Option<f64> {
    let (a, b) = (centre.saturating_sub(60), (centre + 60).min(env.len()));
    if a >= b {
        return None;
    }
    let pk = (a..b).max_by(|&i, &j| env[i].partial_cmp(&env[j]).unwrap())?;
    let th = 0.1 * env[pk];
    let mut i = pk;
    while i > 0 && env[i] > th {
        i -= 1;
    }
    Some((pk - i) as f64 * HOP as f64 / SR as f64 * 1000.0)
}

pub struct Smear {
    pub ncc: f64,
    pub lag_ms: f64,
    pub rise_orig_ms: f64,
    pub rise_out_ms: f64,
}

pub fn transient_smear(orig: &[f32], out: &[f32], tempo: f64) -> Smear {
    let eo = rms_env(orig);
    let ey = rms_env(out);
    let oo = onset_env(&eo);
    let oy = onset_env(&ey);
    let ow = warp(&oo, tempo);
    let (r, lag) = ncc(&ow, &oy, 40);
    let marks = onsets(&oo);
    let (mut so, mut sy, mut cnt) = (0.0, 0.0, 0);
    for &m in &marks {
        let mo = rise_ms(&eo, m);
        let my = rise_ms(&ey, (m as f64 / tempo) as usize);
        if let (Some(a), Some(b)) = (mo, my) {
            so += a;
            sy += b;
            cnt += 1;
        }
    }
    let c = cnt.max(1) as f64;
    Smear { ncc: r, lag_ms: lag as f64 * HOP as f64 / SR as f64 * 1000.0, rise_orig_ms: so / c, rise_out_ms: sy / c }
}

// ---------- eval driver ----------
/// Decorrelated stereo from a mono source: L = x, R = 0.9 * x delayed by 331 samples (7.5 ms).
pub fn make_stereo(x: &[f32]) -> Vec<f32> {
    let d = 331;
    let mut o = Vec::with_capacity(x.len() * 2);
    for i in 0..x.len() {
        o.push(x[i]);
        o.push(if i >= d { 0.9 * x[i - d] } else { 0.0 });
    }
    o
}

fn median(v: &mut Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

pub fn eval_all() {
    let out_dir = Path::new("out");
    let chord = synth::chord(8.0);
    let drums = synth::drums(8.0);
    let sine = synth::sine(440.0, 8.0);
    write_wav_mono(&out_dir.join("chord_orig.wav"), &chord, SR);
    write_wav_mono(&out_dir.join("drums_orig.wav"), &drums, SR);
    let secs = 8.0;
    let runs = 5;
    let (mut pt, mut tt, mut ct) = (String::new(), String::new(), String::new());
    writeln!(pt, "| engine | tempo | sine 440 Hz, FFT peak (cents) | chord, centroid (cents, max of 3) | chord, FFT peak (cents, max of 3) |\n|---|---|---|---|---|").unwrap();
    writeln!(tt, "| engine | tempo | onset NCC | best lag (ms) | attack rise orig -> out (ms) |\n|---|---|---|---|---|").unwrap();
    writeln!(ct, "| engine | tempo | mono ms CPU per audio-s (chord / drums) | mono % core | STEREO ms CPU per audio-s (chord / drums) | STEREO % core per deck (worst) |\n|---|---|---|---|---|---|").unwrap();
    for e in engines::all() {
        for &tempo in &[0.92f64, 1.08] {
            let mut cpu = [0.0f64; 2];
            let mut res: [Vec<f32>; 2] = [vec![], vec![]];
            for (mi, (name, x)) in [("chord", &chord), ("drums", &drums)].iter().enumerate() {
                let _ = (e.run)(x, 1, tempo); // warm-up
                let mut ts = vec![];
                for _ in 0..runs {
                    let t0 = Instant::now();
                    let y = (e.run)(x, 1, tempo);
                    ts.push(t0.elapsed().as_secs_f64());
                    res[mi] = y;
                }
                cpu[mi] = median(&mut ts) / secs * 1000.0;
                write_wav_mono(&out_dir.join(format!("{name}_r{tempo:.2}_{}.wav", e.name)), &res[mi], SR);
            }
            let mut scpu = [0.0f64; 2];
            for (mi, x) in [&chord, &drums].iter().enumerate() {
                let st = make_stereo(x);
                let _ = (e.run)(&st, 2, tempo);
                let mut ts = vec![];
                for _ in 0..runs {
                    let t0 = Instant::now();
                    let y = (e.run)(&st, 2, tempo);
                    ts.push(t0.elapsed().as_secs_f64());
                    assert_eq!(y.len() % 2, 0);
                }
                scpu[mi] = median(&mut ts) / secs * 1000.0;
            }
            let sworst = scpu[0].max(scpu[1]);
            let ysine = (e.run)(&sine, 1, tempo);
            let p_sine = pitch_error_cents(&sine, &ysine, &[440.0]);
            let p_cen = pitch_error_centroid(&chord, &res[0], &synth::CHORD_HZ);
            let p_pk = pitch_error_cents(&chord, &res[0], &synth::CHORD_HZ);
            let sm = transient_smear(&drums, &res[1], tempo);
            let worst = cpu[0].max(cpu[1]);
            writeln!(pt, "| {} | {tempo:.2} | {p_sine:.2} | {p_cen:.2} | {p_pk:.2} |", e.name).unwrap();
            writeln!(tt, "| {} | {tempo:.2} | {:.3} | {:+.1} | {:.1} -> {:.1} |", e.name, sm.ncc, sm.lag_ms, sm.rise_orig_ms, sm.rise_out_ms).unwrap();
            writeln!(ct, "| {} | {tempo:.2} | {:.1} / {:.1} | {:.2} | {:.1} / {:.1} | {:.2} |", e.name, cpu[0], cpu[1], worst / 10.0, scpu[0], scpu[1], sworst / 10.0).unwrap();
            eprintln!("done {} @ {tempo}", e.name);
        }
    }
    let md = format!("### Pitch\n\n{pt}\n### Transients\n\n{tt}\n### CPU\n\n{ct}");
    println!("{md}");
    std::fs::write(out_dir.join("results.md"), &md).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_metric_detects_known_shift() {
        let a = synth::sine(440.0, 3.0);
        let same = pitch_error_cents(&a, &a, &[440.0]);
        assert!(same < 1e-9);
        // a 1% frequency shift is 17.23 cents
        let b = synth::sine(440.0 * 1.01, 3.0);
        let e = pitch_error_cents(&a, &b, &[440.0]);
        assert!((e - 17.23).abs() < 1.0, "got {e}");
        // measurement itself accurate on an off-bin tone
        let hz = peak_hz(&synth::sine(300.37, 3.0), 300.0);
        assert!(cents(hz, 300.37).abs() < 1.0, "got {hz}");
    }

    #[test]
    fn identity_warp_gives_ncc_one() {
        let d = synth::drums(4.0);
        let o = onset_env(&rms_env(&d));
        let (r, lag) = ncc(&warp(&o, 1.0), &o, 40);
        assert!(r > 0.999 && lag == 0);
        assert!(onsets(&o).len() >= 8);
    }

    #[test]
    fn smear_metric_penalises_blur() {
        let d = synth::drums(4.0);
        // moving-average blur of 20 ms smears attacks
        let w = 882;
        let blurred: Vec<f32> = (0..d.len())
            .map(|i| {
                let (a, b) = (i.saturating_sub(w), i);
                d[a..=b].iter().sum::<f32>() / (b - a + 1) as f32
            })
            .collect();
        let s0 = transient_smear(&d, &d, 1.0);
        let s1 = transient_smear(&d, &blurred, 1.0);
        assert!(s0.ncc > 0.999);
        assert!(s1.ncc < s0.ncc && s1.rise_out_ms > s1.rise_orig_ms, "{} {}", s1.ncc, s1.rise_out_ms);
    }

    #[test]
    fn ratio_one_preserves_length_for_every_engine() {
        let x = synth::chord(2.0);
        for e in engines::all() {
            let y = (e.run)(&x, 1, 1.0);
            let d = (y.len() as f64 - x.len() as f64).abs() / x.len() as f64;
            assert!(d < 0.005, "{}: {} vs {}", e.name, y.len(), x.len());
            assert!(y.iter().all(|v| v.is_finite()), "{} non-finite", e.name);
        }
    }

    #[test]
    fn stereo_runs_and_keeps_channels_distinct_for_every_engine() {
        let m = synth::chord(2.0);
        let st = make_stereo(&m);
        for e in engines::all() {
            let y = (e.run)(&st, 2, 1.08);
            let frames = y.len() / 2;
            let want = m.len() as f64 / 1.08;
            assert!((frames as f64 - want).abs() / want < 0.005, "{}", e.name);
            assert!(y.iter().all(|v| v.is_finite()), "{}", e.name);
            let (l, r): (Vec<f32>, Vec<f32>) = (y.iter().step_by(2).copied().collect(), y.iter().skip(1).step_by(2).copied().collect());
            let e_l: f64 = l.iter().map(|v| (*v as f64).powi(2)).sum();
            let e_r: f64 = r.iter().map(|v| (*v as f64).powi(2)).sum();
            assert!(e_l > 0.0 && e_r > 0.0, "{} silent channel", e.name);
        }
    }

    #[test]
    fn ratio_changes_length_for_every_engine() {
        let x = synth::chord(2.0);
        for e in engines::all() {
            let y = (e.run)(&x, 1, 1.08);
            let want = x.len() as f64 / 1.08;
            assert!((y.len() as f64 - want).abs() / want < 0.005, "{}", e.name);
        }
    }
}

/// Diagnostic: per-component peak frequencies, for chasing metric anomalies.
pub fn debug_pitch() {
    let chord = synth::chord(8.0);
    for e in engines::all() {
        for &t in &[0.92f64, 1.08] {
            let y = (e.run)(&chord, 1, t);
            let v: Vec<String> = synth::CHORD_HZ
                .iter()
                .map(|&f| format!("{:.2}->{:.2}({:+.1}c)", peak_hz(&chord, f), peak_hz(&y, f), cents(peak_hz(&y, f), peak_hz(&chord, f))))
                .collect();
            println!("{} {t}: {}", e.name, v.join("  "));
        }
    }
}

/// Diagnostic: spectrum bins around 440 Hz for one engine on the pure sine.
pub fn debug_sine(name: &str, tempo: f64) {
    let e = engines::all().into_iter().find(|e| e.name == name).unwrap();
    let x = synth::sine(440.0, 8.0);
    let y = (e.run)(&x, 1, tempo);
    println!("peak {:.3} centroid {:.3}", peak_hz(&y, 440.0), centroid_hz(&y, 440.0));
    // amplitude over time of the output (200 ms blocks)
    for c in y.chunks(8820).take(10) {
        let rms = (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt();
        print!("{rms:.3} ");
    }
    println!();
    // zero-crossing frequency estimate per 1 s block
    for c in y.chunks(44100).take(6) {
        let z = c.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        print!("{z} ");
    }
    println!();
}

/// Diagnostic: Rubber Band R2 pitch on the pure sine under different option flags.
pub fn debug_rb_opts() {
    use rubberband::{Options, Stretcher};
    let x = synth::sine(440.0, 8.0);
    let tempo = 1.08;
    let variants: Vec<(&str, Options)> = vec![
        ("default", Options::empty()),
        ("laminar", Options::PHASE_LAMINAR),
        ("independent", Options::PHASE_INDEPENDENT),
        ("smooth", Options::TRANSIENTS_SMOOTH),
        ("long", Options::WINDOW_LONG),
        ("highconsistency", Options::PITCH_HIGH_CONSISTENCY),
        ("highquality", Options::PITCH_HIGH_QUALITY),
        ("smoothing", Options::SMOOTHING_ON),
    ];
    for (n, v) in variants {
        let o = Options::THREADING_NEVER | Options::PROCESS_OFFLINE | Options::ENGINE_FASTER | v;
        let mut st = Stretcher::new(SR, 1, o, 1.0 / tempo, 1.0);
        st.set_expected_input_duraction(x.len() as u32);
        for c in x.chunks(512) { st.study(&[c], false); }
        st.study(&[&[][..]], true);
        let mut out = vec![];
        let mut buf = vec![0.0f32; 8192];
        for c in x.chunks(512) {
            st.process(&[c], false);
            while let Some(a) = st.available() { if a == 0 { break; } let n = (a as usize).min(8192); let g = st.retrieve(&mut [&mut buf[..n]]) as usize; out.extend_from_slice(&buf[..g]); }
        }
        st.process(&[&[][..]], true);
        while let Some(a) = st.available() { if a == 0 { break; } let n = (a as usize).min(8192); let g = st.retrieve(&mut [&mut buf[..n]]) as usize; out.extend_from_slice(&buf[..g]); }
        println!("{n}: len {} peak {:.2} Hz", out.len(), peak_hz(&out, 440.0));
    }
}
