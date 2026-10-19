//! THROWAWAY SPIKE: one function per candidate, mono f32 at 44.1 kHz, `tempo` = playback-rate factor
//! (output length = input length / tempo, pitch preserved). All offline, all single-threaded.
use crate::SR;

pub struct Engine {
    pub name: &'static str,
    pub desc: &'static str,
    /// Interleaved f32 in/out, `ch` channels (1 or 2), tempo factor.
    pub run: fn(&[f32], usize, f64) -> Vec<f32>,
}

pub fn all() -> Vec<Engine> {
    vec![
        Engine { name: "wsola", desc: "wsola 0.2.0 (pure Rust, streaming push/pull in 512-sample chunks, default cfg)", run: run_wsola },
        Engine { name: "timestretch-keylock", desc: "timestretch 0.15.0 real-time Engine, EngineProfile::Keylock, 512-frame callbacks, no pre-analysis", run: |x, c, t| run_ts_engine(x, c, t, timestretch::engine::EngineProfile::Keylock) },
        Engine { name: "timestretch-wide", desc: "timestretch 0.15.0 real-time Engine, EngineProfile::WideKeylock (phase vocoder), 512-frame callbacks, no pre-analysis", run: |x, c, t| run_ts_engine(x, c, t, timestretch::engine::EngineProfile::WideKeylock) },
        Engine { name: "timestretch-batch", desc: "timestretch 0.15.0 batch stretch() (with whole-file pre-analysis), MaxQuality-style offline render", run: run_ts_batch },
        Engine { name: "signalsmith", desc: "signalsmith-stretch 0.1.3 (C++ Signalsmith Stretch via cc, NOT pure Rust), preset_default, exact()", run: run_signalsmith },
        Engine { name: "rubberband-rt", desc: "Rubber Band 4.0.0 (vendored C++ via rubberband-sys), real-time mode, R2 engine, 512-sample blocks", run: |x, c, t| run_rb(x, c, t, false, false, rubberband::Options::empty()) },
        Engine { name: "rubberband-rt-r3", desc: "Rubber Band 4.0.0 real-time mode, ENGINE_FINER (R3)", run: |x, c, t| run_rb(x, c, t, false, true, rubberband::Options::empty()) },
        Engine { name: "rubberband-offline", desc: "Rubber Band 4.0.0 OFFLINE mode (study + process), R2 - quality reference, not usable live", run: |x, c, t| run_rb(x, c, t, true, false, rubberband::Options::empty()) },
        Engine { name: "rubberband-rt-smoothing", desc: "Rubber Band 4.0.0 real-time, R2 + SMOOTHING_ON (found to remove the steady-tone pitch drift of the defaults)", run: |x, c, t| run_rb(x, c, t, false, false, rubberband::Options::SMOOTHING_ON) },
        Engine { name: "rubberband-rt-mixed", desc: "Rubber Band 4.0.0 real-time, R2 + TRANSIENTS_MIXED + SMOOTHING_ON", run: |x, c, t| run_rb(x, c, t, false, false, rubberband::Options::TRANSIENTS_MIXED | rubberband::Options::SMOOTHING_ON) },
    ]
}

/// Expected output length in FRAMES.
fn expected_len(n_frames: usize, tempo: f64) -> usize {
    (n_frames as f64 / tempo).round() as usize
}

fn run_wsola(x: &[f32], ch: usize, tempo: f64) -> Vec<f32> {
    use std::num::{NonZeroU16, NonZeroU32};
    let mut ts = wsola::TimeStretch::new(NonZeroU32::new(SR).unwrap(), NonZeroU16::new(ch as u16).unwrap()).unwrap();
    ts.set_tempo(tempo as f32);
    let mut out = Vec::with_capacity((expected_len(x.len() / ch, tempo) + 4096) * ch);
    for c in x.chunks(512 * ch) {
        ts.push(c);
        let avail = ts.available();
        out.extend(ts.pull(avail));
    }
    out.extend(ts.flush());
    out.truncate(expected_len(x.len() / ch, tempo) * ch);
    out
}

fn run_ts_engine(x: &[f32], ch: usize, tempo: f64, profile: timestretch::engine::EngineProfile) -> Vec<f32> {
    use timestretch::engine::{Engine, EngineConfig};
    let h = Engine::build(EngineConfig {
        sample_rate: SR,
        channels: ch,
        profile,
        initial_tempo_rate: tempo,
        max_block_frames: 512,
        ..EngineConfig::default()
    })
    .unwrap();
    let (ctl, mut proc_, mut src) = (h.controller, h.processor, h.source);
    src.set_track_position(0);
    let latency = proc_.pipeline_latency_frames();
    let want = expected_len(x.len() / ch, tempo);
    let flush = vec![0.0f32; (proc_.varispeed_lookahead_frames() + 8192) * ch];
    let total = (want + latency) * ch;
    let mut fed = 0usize;
    let mut flushed = 0usize;
    let mut finished = false;
    let mut out = vec![0.0f32; 512 * ch];
    let mut col: Vec<f32> = Vec::with_capacity(total + 512);
    while col.len() < total {
        while fed < x.len() && src.free_frames() > 0 {
            let end = (fed + 4096 * ch).min(x.len());
            fed += src.push(&x[fed..end]) * ch;
        }
        if fed >= x.len() {
            while flushed < flush.len() && src.free_frames() > 0 {
                flushed += src.push(&flush[flushed..]) * ch;
            }
            if flushed >= flush.len() && !finished {
                finished = src.finish();
            }
        }
        let u0 = ctl.underrun_frames();
        proc_.process(&mut out);
        col.extend_from_slice(&out);
        if finished && ctl.underrun_frames() > u0 {
            break;
        }
    }
    col.drain(..(latency * ch).min(col.len()));
    col.resize(want * ch, 0.0);
    col
}

fn run_ts_batch(x: &[f32], ch: usize, tempo: f64) -> Vec<f32> {
    let p = timestretch::StretchParams::new(1.0 / tempo).with_sample_rate(SR).with_channels(ch as u32);
    let mut y = timestretch::stretch(x, &p).expect("timestretch");
    y.resize(expected_len(x.len() / ch, tempo) * ch, 0.0);
    y
}

fn run_signalsmith(x: &[f32], ch: usize, tempo: f64) -> Vec<f32> {
    let mut s = signalsmith_stretch::Stretch::preset_default(ch as u32, SR);
    let mut out = vec![0.0f32; expected_len(x.len() / ch, tempo) * ch];
    s.exact(x, &mut out);
    out
}

fn run_rb(x: &[f32], ch: usize, tempo: f64, offline: bool, r3: bool, extra: rubberband::Options) -> Vec<f32> {
    use rubberband::{Options, Stretcher};
    let mut o = Options::THREADING_NEVER | extra;
    o |= if offline { Options::PROCESS_OFFLINE } else { Options::PROCESS_REALTIME };
    o |= if r3 { Options::ENGINE_FINER } else { Options::ENGINE_FASTER };
    let frames = x.len() / ch;
    let want = expected_len(frames, tempo);
    // Rubber Band wants planar channels.
    let planar: Vec<Vec<f32>> = (0..ch).map(|c| (0..frames).map(|i| x[i * ch + c]).collect()).collect();
    let mut st = Stretcher::new(SR, ch as u32, o, 1.0 / tempo, 1.0);
    let mut outp: Vec<Vec<f32>> = vec![Vec::with_capacity(want + 8192); ch];
    let mut bufs: Vec<Vec<f32>> = vec![vec![0.0f32; 8192]; ch];
    fn drain(st: &mut Stretcher, outp: &mut [Vec<f32>], bufs: &mut [Vec<f32>]) {
        while let Some(a) = st.available() {
            if a == 0 {
                break;
            }
            let n = (a as usize).min(8192);
            let got = {
                let mut views: Vec<&mut [f32]> = bufs.iter_mut().map(|b| &mut b[..n]).collect();
                st.retrieve(&mut views) as usize
            };
            for c in 0..outp.len() {
                outp[c].extend_from_slice(&bufs[c][..got]);
            }
            if got == 0 {
                break;
            }
        }
    }
    let slices = |a: usize, b: usize| -> Vec<&[f32]> { planar.iter().map(|p| &p[a..b]).collect() };
    let empty: Vec<&[f32]> = (0..ch).map(|_| &[][..]).collect();
    let mut delay = 0;
    if offline {
        st.set_expected_input_duraction(frames as u32);
        for a in (0..frames).step_by(512) {
            st.study(&slices(a, (a + 512).min(frames)), false);
        }
        st.study(&empty, true);
        for a in (0..frames).step_by(512) {
            st.process(&slices(a, (a + 512).min(frames)), false);
            drain(&mut st, &mut outp, &mut bufs);
        }
        st.process(&empty, true);
        drain(&mut st, &mut outp, &mut bufs);
    } else {
        let pad = st.preferred_start_pad() as usize;
        delay = st.start_delay() as usize;
        let zeros = vec![0.0f32; pad.max(4096)];
        let zs: Vec<&[f32]> = (0..ch).map(|_| &zeros[..pad]).collect();
        st.process(&zs, false);
        for a in (0..frames).step_by(512) {
            st.process(&slices(a, (a + 512).min(frames)), false);
            drain(&mut st, &mut outp, &mut bufs);
        }
        let zt: Vec<&[f32]> = (0..ch).map(|_| &zeros[..4096.min(zeros.len())]).collect();
        st.process(&zt, false);
        drain(&mut st, &mut outp, &mut bufs);
        st.process(&empty, true);
        drain(&mut st, &mut outp, &mut bufs);
    }
    let mut out = vec![0.0f32; want * ch];
    for c in 0..ch {
        for i in 0..want {
            out[i * ch + c] = outp[c].get(i + delay).copied().unwrap_or(0.0);
        }
    }
    out
}
