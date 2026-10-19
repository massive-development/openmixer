//! THROWAWAY SPIKE (Task 7): cpal output callback. Not production code.
//!
//! `render` is pure (no device, no alloc, no locks) so it is testable headless.
//! `cargo run --release -- [seconds] [--cue]` streams to the default device and
//! reports callback count / inter-callback jitter / xruns; with `--cue` it also
//! tries to open a second output stream (880 Hz) as a headphone-cue probe.

#[cfg(debug_assertions)]
use assert_no_alloc::AllocDisabler;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, ErrorKind, Stream, StreamConfig};
use rtrb::Consumer;
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
#[global_allocator]
static A: AllocDisabler = AllocDisabler;

pub const FRAMES: u32 = 256;

/// Silent by default: device streams output digital zeros unless the USER sets
/// OPENMIXER_AUDIBLE=1 (then a quiet tone, gain <= AUDIBLE_MAX_GAIN).
pub const AUDIBLE_MAX_GAIN: f32 = 0.05;
fn audible() -> bool {
    std::env::var("OPENMIXER_AUDIBLE").is_ok_and(|v| v == "1")
}

/// Commands sent control thread -> audio thread over the SPSC ring.
#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    SetGain(f32),
}

pub struct State {
    phase: f32,
    inc: f32,
    gain: f32,
}

impl State {
    pub fn new(sample_rate: f32) -> Self {
        Self::with_freq(sample_rate, 440.0)
    }
    pub fn with_freq(sample_rate: f32, hz: f32) -> Self {
        State {
            phase: 0.0,
            inc: hz / sample_rate,
            gain: 0.5,
        }
    }
}

/// Fill `out` (interleaved, `channels` per frame). No allocation, no locks.
pub fn render(out: &mut [f32], channels: usize, state: &mut State, cmds: &mut Consumer<Cmd>) {
    while let Ok(c) = cmds.pop() {
        match c {
            Cmd::SetGain(g) => state.gain = g,
        }
    }
    for frame in out.chunks_mut(channels) {
        let v = (state.phase * TAU).sin() * state.gain;
        state.phase += state.inc;
        if state.phase >= 1.0 {
            state.phase -= 1.0;
        }
        frame.fill(v);
    }
}

#[derive(Default)]
pub struct Stats {
    callbacks: AtomicU64,
    last_ns: AtomicU64,
    max_gap_ns: AtomicU64,
    sum_gap_ns: AtomicU64,
    late: AtomicU64, // gap > 1.5 x nominal period
    xruns: AtomicU64,
    other_errors: AtomicU64,
}

fn open(
    device: &Device,
    hz: f32,
    buffer: BufferSize,
    stats: Arc<Stats>,
    mut cons: Consumer<Cmd>,
) -> Result<(Stream, StreamConfig), String> {
    let def = device
        .default_output_config()
        .map_err(|e| format!("default config: {e}"))?;
    let mut cfg: StreamConfig = def.into();
    cfg.buffer_size = buffer;
    let channels = cfg.channels as usize;
    let sr = cfg.sample_rate as f32;
    let period_ns = (FRAMES as f64 / sr as f64 * 1e9) as u64;
    let t0 = Instant::now();
    let mut state = State::with_freq(sr, hz);
    let audible = audible();
    state.gain = if audible { AUDIBLE_MAX_GAIN } else { 0.0 };
    let st = stats.clone();
    let err_stats = stats;
    let stream = device
        .build_output_stream::<f32, _, _>(
            cfg,
            move |data: &mut [f32], _| {
                let now = t0.elapsed().as_nanos() as u64;
                let last = st.last_ns.swap(now, Relaxed);
                if st.callbacks.fetch_add(1, Relaxed) > 0 {
                    let gap = now - last;
                    st.sum_gap_ns.fetch_add(gap, Relaxed);
                    st.max_gap_ns.fetch_max(gap, Relaxed);
                    if gap > period_ns * 3 / 2 {
                        st.late.fetch_add(1, Relaxed);
                    }
                }
                render(data, channels, &mut state, &mut cons);
                if !audible {
                    data.fill(0.0); // digital silence on real devices
                }
            },
            move |e| match e.kind() {
                ErrorKind::Xrun => {
                    err_stats.xruns.fetch_add(1, Relaxed);
                }
                _ => {
                    err_stats.other_errors.fetch_add(1, Relaxed);
                    eprintln!("stream error: {e}");
                }
            },
            None,
        )
        .map_err(|e| format!("build_output_stream (f32, {buffer:?}): {e}"))?;
    Ok((stream, cfg))
}

fn report(name: &str, s: &Stats, cfg: &StreamConfig, secs: f64) {
    let n = s.callbacks.load(Relaxed);
    let period_ms = FRAMES as f64 / cfg.sample_rate as f64 * 1e3;
    let mean_ms = if n > 1 {
        s.sum_gap_ns.load(Relaxed) as f64 / (n - 1) as f64 / 1e6
    } else {
        0.0
    };
    println!(
        "[{name}] {secs:.0}s ch={} sr={} callbacks={n} (expected ~{:.0}) period={period_ms:.3}ms mean_gap={mean_ms:.3}ms max_gap={:.3}ms late(>1.5x)={} xruns={} other_errors={}",
        cfg.channels, cfg.sample_rate,
        secs * 1e3 / period_ms,
        s.max_gap_ns.load(Relaxed) as f64 / 1e6,
        s.late.load(Relaxed), s.xruns.load(Relaxed), s.other_errors.load(Relaxed),
    );
}

#[cfg(all(feature = "pulse", target_os = "linux"))]
fn pick_host(pulse: bool) -> cpal::Host {
    if pulse {
        cpal::host_from_id(cpal::HostId::PulseAudio).expect("pulse host")
    } else {
        cpal::default_host()
    }
}
#[cfg(not(all(feature = "pulse", target_os = "linux")))]
fn pick_host(_pulse: bool) -> cpal::Host {
    cpal::default_host()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: u64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(60);
    let cue = args.iter().any(|a| a == "--cue");
    let host = pick_host(args.iter().any(|a| a == "--pulse"));
    println!("host: {}", host.id());
    let Some(dev) = host.default_output_device() else {
        println!("no default output device; nothing to run (pending user)");
        return;
    };
    println!(
        "device: {}",
        dev.id().map(|i| i.to_string()).unwrap_or_default()
    );
    let (mut prod, cons) = rtrb::RingBuffer::<Cmd>::new(64);
    let s1 = Arc::new(Stats::default());
    let (stream, cfg) = match open(&dev, 440.0, BufferSize::Fixed(FRAMES), s1.clone(), cons) {
        Ok(x) => x,
        Err(e) => {
            println!("FAILED to open main stream: {e}");
            return;
        }
    };
    stream.play().expect("play");

    let mut second = None;
    if cue {
        // Probe: second stream. Prefer a different device; else the same device.
        let devs: Vec<Device> = host
            .output_devices()
            .map(|d| d.collect())
            .unwrap_or_default();
        let id1 = dev.id().ok();
        let (other, label) = match devs.into_iter().find(|d| d.id().ok() != id1) {
            Some(d) => (d, "second device"),
            None => match host.default_output_device() {
                Some(d) => (d, "same device"),
                None => {
                    println!("no default output device");
                    return;
                }
            },
        };
        let (_p2, c2) = rtrb::RingBuffer::<Cmd>::new(8);
        let s2 = Arc::new(Stats::default());
        let mut r = open(&other, 880.0, BufferSize::Fixed(FRAMES), s2.clone(), c2);
        if let Err(e) = &r {
            println!("cue: Fixed({FRAMES}) failed on {label}: {e}; retrying BufferSize::Default");
            let (_p3, c3) = rtrb::RingBuffer::<Cmd>::new(8);
            r = open(&other, 880.0, BufferSize::Default, s2.clone(), c3);
        }
        match r {
            Ok((st2, cfg2)) => match st2.play() {
                Ok(()) => {
                    println!("cue: opened + playing on {label}");
                    second = Some((st2, s2, cfg2));
                }
                Err(e) => println!("cue: opened on {label} but play() failed: {e}"),
            },
            Err(e) => println!("cue: could not open second stream on {label}: {e}"),
        }
    }

    // Gain change mid-run via the ring, as a sanity check of the command path.
    std::thread::sleep(Duration::from_secs(secs / 2));
    let _ = prod.push(Cmd::SetGain(AUDIBLE_MAX_GAIN / 2.0));
    std::thread::sleep(Duration::from_secs(secs - secs / 2));
    drop(stream);
    report("main", &s1, &cfg, secs as f64);
    if let Some((st2, s2, cfg2)) = second {
        drop(st2);
        report("cue", &s2, &cfg2, secs as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_no_alloc::assert_no_alloc;

    const SR: f32 = 48_000.0;
    const FRAMES: usize = 256;

    #[test]
    fn device_opens_or_skips() {
        let host = cpal::default_host();
        let Some(dev) = host.default_output_device() else {
            println!(
                "SKIP: no default output device on this host ({})",
                host.id()
            );
            return;
        };
        // ALSA reports a virtual "default" device even with no sound card, so also
        // skip when it cannot give us a config (i.e. nothing real behind it).
        if let Err(e) = dev.default_output_config() {
            println!("SKIP: default output device has no usable config: {e}");
            return;
        }
        let (_p, c) = rtrb::RingBuffer::<Cmd>::new(8);
        let stats = Arc::new(Stats::default());
        let (stream, _cfg) = open(
            &dev,
            440.0,
            BufferSize::Fixed(super::FRAMES as u32),
            stats.clone(),
            c,
        )
        .expect("open");
        stream.play().expect("play");
        std::thread::sleep(Duration::from_millis(500));
        drop(stream);
        assert!(stats.callbacks.load(Relaxed) > 0, "no callbacks fired");
    }

    #[test]
    fn callback_does_not_allocate() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<Cmd>::new(64);
        let mut state = State::new(SR);
        let channels = 2;
        let mut buf = vec![0.0f32; FRAMES * channels];
        let mut peak_before = 0.0f32;
        let mut peak_after = 0.0f32;
        for i in 0..1000 {
            if i == 500 {
                prod.push(Cmd::SetGain(0.25)).unwrap();
            }
            assert_no_alloc(|| render(&mut buf, channels, &mut state, &mut cons));
            if i == 499 {
                peak_before = buf.iter().fold(0.0, |m, s| s.abs().max(m));
            }
            if i == 999 {
                peak_after = buf.iter().fold(0.0, |m, s| s.abs().max(m));
            }
        }
        assert!(
            peak_before > 0.4,
            "pre-gain peak {peak_before} (initial gain 0.5)"
        );
        assert!(peak_after > 0.2 && peak_after <= 0.26, "peak {peak_after}");
    }
}
