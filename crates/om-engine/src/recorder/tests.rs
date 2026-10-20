//! Recorder tests. Recordings are user-visible files, so every test writes only to a temp dir.

use super::*;
use crate::callback::AudioCallback;
use crate::dj_engine::DjEngine;
use crate::sampler::Sample;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

const RATE: f64 = 44_100.0;
const BLOCK: usize = 256;

fn settings(dir: &Path) -> RecordSettings {
    let mut s = RecordSettings::new(dir.join("rec"));
    s.free_space = Arc::new(|_| Some(u64::MAX));
    s.header_interval = Duration::from_millis(20);
    s.flush_interval = Duration::from_millis(10);
    s
}

fn prepared() -> (RecorderTap, RecorderControl) {
    let (mut tap, control) = Recorder::create();
    tap.prepare(RATE);
    (tap, control)
}

/// A distinct, bounded signal per channel.
fn signal(n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let t = i as f32;
            (0.8 * (t * 0.031).sin(), 0.6 * (t * 0.017).cos())
        })
        .collect()
}

fn feed(tap: &mut RecorderTap, frames: &[(f32, f32)]) {
    for chunk in frames.chunks(BLOCK) {
        let l: Vec<f32> = chunk.iter().map(|f| f.0).collect();
        let r: Vec<f32> = chunk.iter().map(|f| f.1).collect();
        tap.process(&l, &r);
    }
}

fn read(path: &Path) -> Vec<f32> {
    let sample = Sample::load(path, RATE).expect("recording decodes");
    sample.data().to_vec()
}

fn flat(frames: &[(f32, f32)]) -> Vec<f32> {
    frames.iter().flat_map(|f| [f.0, f.1]).collect()
}

fn max_err(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "length");
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

fn wait(mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn recorded_signal_matches_within_the_format_step() {
    for (format, tol) in [
        (RecordFormat::Pcm24, 1.5e-7),
        (RecordFormat::Pcm16, 5.0e-5),
        (RecordFormat::Float32, 0.0),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (mut tap, control) = prepared();
        let mut s = settings(dir.path());
        s.format = format;
        control.start(&s).unwrap();
        let input = signal(40_000);
        feed(&mut tap, &input);
        let summary = control.stop().unwrap();
        assert_eq!(summary.frames, 40_000);
        assert_eq!(summary.dropped_frames, 0);
        assert_eq!(summary.files.len(), 1);
        let got = read(&summary.files[0]);
        assert!(max_err(&got, &flat(&input)) <= tol, "{format:?}");
    }
}

#[test]
fn header_describes_each_format() {
    for (format, tag, bits) in [
        (RecordFormat::Pcm16, 1u16, 16u16),
        (RecordFormat::Pcm24, 1, 24),
        (RecordFormat::Float32, 3, 32),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (mut tap, control) = prepared();
        let mut s = settings(dir.path());
        s.format = format;
        control.start(&s).unwrap();
        feed(&mut tap, &signal(1000));
        let summary = control.stop().unwrap();
        let bytes = std::fs::read(&summary.files[0]).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), tag);
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), bits);
        let data = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(data, 1000 * format.frame_bytes() as usize);
        assert_eq!(bytes.len(), 44 + data);
    }
}

#[test]
fn the_default_format_is_24_bit() {
    assert_eq!(RecordFormat::default(), RecordFormat::Pcm24);
    assert_eq!(RecordFormat::parse("float32"), Some(RecordFormat::Float32));
    assert_eq!(RecordFormat::parse("mp3"), None);
}

#[test]
fn a_crash_leaves_a_playable_file() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    control.start(&settings(dir.path())).unwrap();
    let input = signal(30_000);
    feed(&mut tap, &input);
    assert!(wait(|| control.status().bytes >= 44 + 30_000 * 6));
    // Let at least one header patch happen after the last write.
    std::thread::sleep(Duration::from_millis(150));
    let file = control.status().file.unwrap();
    control.crash_for_test();
    let got = read(&file);
    assert_eq!(
        got.len(),
        60_000,
        "no stop() was called, yet all frames are there"
    );
    assert!(max_err(&got, &flat(&input)) <= 1.5e-7);
}

#[test]
fn files_split_without_losing_or_repeating_a_frame() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    let mut s = settings(dir.path());
    s.format = RecordFormat::Float32;
    s.max_data_bytes = 8 * 1000; // 1000 frames per file
    s.file_stem = Some("split".into());
    control.start(&s).unwrap();
    let input = signal(4_500);
    feed(&mut tap, &input);
    let summary = control.stop().unwrap();
    assert_eq!(summary.files.len(), 5);
    let names: Vec<_> = summary
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "split.wav",
            "split-part2.wav",
            "split-part3.wav",
            "split-part4.wav",
            "split-part5.wav"
        ]
    );
    let mut joined = Vec::new();
    for (i, f) in summary.files.iter().enumerate() {
        let part = read(f);
        assert_eq!(part.len(), if i < 4 { 2000 } else { 1000 }, "part {i}");
        joined.extend(part);
    }
    assert_eq!(joined, flat(&input));
}

#[test]
fn a_full_ring_drops_and_counts_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    let stall = Arc::new(AtomicBool::new(true));
    let mut s = settings(dir.path());
    s.stall = Some(Arc::clone(&stall));
    s.format = RecordFormat::Float32;
    control.start(&s).unwrap();
    let started = Instant::now();
    // 256 000 frames into a 192 000-frame ring.
    feed(&mut tap, &signal(1000 * BLOCK));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the tap blocked"
    );
    assert_eq!(
        control.status().dropped_frames,
        1000 * BLOCK as u64 - 192_000
    );
    stall.store(false, Relaxed);
    let summary = control.stop().unwrap();
    assert_eq!(summary.frames, 192_000);
    assert_eq!(summary.dropped_frames, 64_000);
    // What was kept is the start of the signal, in order.
    let got = read(&summary.files[0]);
    assert_eq!(got, flat(&signal(192_000)));
}

#[test]
fn off_means_nothing_is_pushed() {
    let (mut tap, control) = prepared();
    feed(&mut tap, &signal(10_000));
    assert_eq!(control.status().dropped_frames, 0);
    assert_eq!(control.status().peak_l, 0.0);
    assert_eq!(tap.tx.slots(), RING_SAMPLES);
}

#[test]
fn start_and_stop_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    assert!(matches!(control.stop(), Err(RecordError::NotRecording)));
    assert_eq!(control.state(), RecordState::Idle);
    let s = settings(dir.path());
    control.start(&s).unwrap();
    assert_eq!(control.state(), RecordState::Recording);
    assert!(matches!(
        control.start(&s),
        Err(RecordError::AlreadyRecording)
    ));
    assert_eq!(control.state(), RecordState::Recording);
    feed(&mut tap, &signal(2000));
    let first = control.stop().unwrap();
    assert!(matches!(control.stop(), Err(RecordError::NotRecording)));
    assert_eq!(control.state(), RecordState::Idle);
    // A second recording works, in a new file, from a clean ring.
    control.start(&s).unwrap();
    feed(&mut tap, &signal(500));
    let second = control.stop().unwrap();
    assert_eq!(second.frames, 500);
    assert_ne!(first.files[0], second.files[0]);
    assert_eq!(read(&first.files[0]).len(), 4000);
}

#[test]
fn bad_folders_fail_and_leave_the_recorder_idle() {
    let dir = tempfile::tempdir().unwrap();
    let (_tap, control) = prepared();
    let blocker = dir.path().join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let mut s = settings(dir.path());
    s.folder = blocker.join("sub");
    assert!(matches!(control.start(&s), Err(RecordError::BadFolder(_))));
    assert_eq!(control.state(), RecordState::Idle);
    s.folder = dir.path().join("a/../b");
    assert!(matches!(control.start(&s), Err(RecordError::BadFolder(_))));
    s.folder = PathBuf::new();
    assert!(matches!(control.start(&s), Err(RecordError::BadFolder(_))));
    assert_eq!(control.state(), RecordState::Idle);
    assert!(!dir.path().join("b").exists());
}

#[test]
fn start_before_the_device_is_prepared_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (_tap, control) = Recorder::create();
    assert!(matches!(
        control.start(&settings(dir.path())),
        Err(RecordError::NotPrepared)
    ));
}

#[test]
fn a_recording_never_overwrites_and_names_stay_in_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    let mut s = settings(dir.path());
    s.file_stem = Some("../../evil name".into());
    control.start(&s).unwrap();
    feed(&mut tap, &signal(300));
    let a = control.stop().unwrap();
    control.start(&s).unwrap();
    feed(&mut tap, &signal(300));
    let b = control.stop().unwrap();
    assert_ne!(a.files[0], b.files[0]);
    for f in a.files.iter().chain(&b.files) {
        assert_eq!(f.parent().unwrap(), dir.path().join("rec"));
    }
    assert_eq!(
        std::fs::read_dir(dir.path().join("rec")).unwrap().count(),
        2
    );
}

#[test]
fn low_disk_refuses_to_start_and_stops_a_running_recording() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    let free = Arc::new(AtomicU64::new(100 * 1024 * 1024));
    let mut s = settings(dir.path());
    let probe = Arc::clone(&free);
    s.free_space = Arc::new(move |_| Some(probe.load(Relaxed)));
    assert!(matches!(control.start(&s), Err(RecordError::LowDisk(_))));
    assert_eq!(control.state(), RecordState::Idle);

    free.store(500 * 1024 * 1024, Relaxed);
    control.start(&s).unwrap();
    assert!(control.status().low_disk_warning, "warns under 1 GB");
    feed(&mut tap, &signal(5000));
    free.store(100 * 1024 * 1024, Relaxed);
    assert!(wait(|| control.state() == RecordState::Error));
    assert!(control.status().error.unwrap().contains("disk space"));
    let summary = control.stop().unwrap();
    assert!(summary.error.is_some());
    assert_eq!(
        read(&summary.files[0]).len(),
        10_000,
        "the file was finalised"
    );
    assert_eq!(control.state(), RecordState::Idle);
}

#[test]
fn file_names_follow_the_pattern() {
    assert_eq!(stem_from_unix(0), "OpenMixer-1970-01-01-000000");
    assert_eq!(stem_from_unix(1_700_000_000), "OpenMixer-2023-11-14-221320");
    assert_eq!(
        stem_from_unix(951_782_400 + 86_399),
        "OpenMixer-2000-02-29-235959"
    );
}

#[test]
fn status_reports_elapsed_and_peaks() {
    let dir = tempfile::tempdir().unwrap();
    let (mut tap, control) = prepared();
    control.start(&settings(dir.path())).unwrap();
    feed(&mut tap, &vec![(0.5, -0.25); 44_100]);
    assert!(wait(|| control.status().elapsed_s >= 1.0));
    let st = control.status();
    assert!((st.elapsed_s - 1.0).abs() < 1e-9);
    assert!((st.peak_l - 0.5).abs() < 1e-6 && (st.peak_r - 0.25).abs() < 1e-6);
    let snap = control.snapshot();
    assert!(snap.recording && !snap.failed);
    control.stop().unwrap();
}

/// The recording is the master output: render a sampler tone through the engine and compare the
/// file with what the (mock) device buffers received.
#[test]
fn recording_equals_the_rendered_master() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, BLOCK);
    let tone: Vec<f32> = (0..30_000)
        .flat_map(|n| {
            let v = 0.5 * ((n as f32) * 0.05).sin();
            [v, -0.5 * v]
        })
        .collect();
    engine
        .sampler()
        .assign(
            0,
            Arc::new(Sample::from_interleaved("tone", RATE, tone).unwrap()),
        )
        .unwrap();
    engine.mixer().set_master_gain(0.7);
    engine.recorder().start(&settings(dir.path())).unwrap();
    engine.sampler().trigger(0, 1.0);
    let mut rendered = Vec::new();
    for _ in 0..100 {
        let (mut a, mut b, mut c, mut d) = (
            vec![0.0; BLOCK],
            vec![0.0; BLOCK],
            vec![0.0; BLOCK],
            vec![0.0; BLOCK],
        );
        audio.process(&[], &mut [&mut a[..], &mut b[..], &mut c[..], &mut d[..]]);
        rendered.extend(a.iter().zip(&b).flat_map(|(l, r)| [*l, *r]));
    }
    assert!(rendered.iter().any(|v| v.abs() > 0.05), "something sounded");
    let summary = engine.recorder().stop().unwrap();
    assert_eq!(summary.dropped_frames, 0);
    let got = read(&summary.files[0]);
    assert!(max_err(&got, &rendered) <= 1.5e-7);
}
