//! The pure part of the waveform frame budget on a six minute demo loop: building the overview
//! columns of two decks, rendering the cached zoomed strips, and the per-frame scroll path
//! (`visible()` plus the bucket runs). Numbers are printed (`-- --nocapture`). The always-on test
//! asserts with 5 times headroom in release builds only (debug builds print); the strict plan
//! numbers (50 ms build, 0.3 ms per strip per frame) are `--ignored`, run by hand with `--release`.

use om_analysis::{demo_loop, AudioClip, NullAnalysisContext, WaveformBuilder, WaveformData};
use om_ui::waveform::{
    bucket_runs, overview_shapes, reference_peak, BandStrip, StripSource, View, MAX_BUCKETS,
};
use std::time::{Duration, Instant};

const RATE: f64 = 44_100.0;
const OVERVIEW_PX: usize = 600;
const STRIP_PX: f64 = 1280.0;
const STRIP_HEIGHT: i32 = 96;
const BUILD_LIMIT: Duration = Duration::from_millis(50);
const FRAME_LIMIT: Duration = Duration::from_micros(300);
const HEADROOM: u32 = 5;

fn six_minutes() -> (WaveformData, i64) {
    let clip = AudioClip::new(demo_loop(360.0, 124.0, RATE), RATE);
    let data = WaveformBuilder::default()
        .build(&clip, &NullAnalysisContext)
        .expect("not cancelled");
    let length = clip.samples.len() as i64;
    (data, length)
}

struct Measured {
    build: Duration,
    frame: Duration,
    paths: usize,
    vertices: usize,
    columns: usize,
    runs: usize,
}

fn measure() -> Measured {
    let (data, length) = six_minutes();
    let started = Instant::now();
    let points = &data.overview().points;
    let reference = reference_peak(points);
    let overviews: Vec<_> = (0..2)
        .map(|_| overview_shapes(points, OVERVIEW_PX / 2, reference, true))
        .collect();
    let view = |centre: i64| View {
        centre_frame: centre,
        frames_per_pixel: 8.0 * RATE / STRIP_PX,
        width: STRIP_PX,
    };
    let level = data.level_for(view(0).frames_per_pixel);
    let src = StripSource {
        track: 1,
        level,
        height_px: STRIP_HEIGHT,
        length_frames: length,
        reference,
        spectral: true,
    };
    let strips: Vec<_> = (0..2)
        .map(|i| BandStrip::render(&src, &view(length / 3 * (i + 1))))
        .collect();
    let build = started.elapsed();
    std::hint::black_box((&overviews, &strips));

    // One frame of scrolling, per strip: the visible window and its bucket runs.
    let strip = &strips[0];
    let mut frame = Duration::MAX;
    let (mut paths, mut vertices, mut columns, mut runs) = (0, 0, 0, 0);
    for attempt in 0..50 {
        let v = view(length / 3 + attempt * 400);
        let started = Instant::now();
        let visible = strip.visible(&v);
        let keys: Vec<_> = visible.iter().map(|(_, c)| c.key).collect();
        let groups = bucket_runs(&keys);
        frame = frame.min(started.elapsed());
        paths = groups.len() + 1;
        columns = visible.len();
        runs = groups.iter().map(|(_, r)| r.len()).sum();
        // Silhouette: two vertices per column; each run adds one shared edge column.
        vertices = 2 * columns
            + groups
                .iter()
                .flat_map(|(_, r)| r)
                .map(|r| 2 * (r.len() + 1))
                .sum::<usize>();
    }
    Measured {
        build,
        frame,
        paths,
        vertices,
        columns,
        runs,
    }
}

#[test]
fn the_waveform_stays_inside_its_budget() {
    let m = measure();
    println!(
        "waveform budget: build {:?} (limit {BUILD_LIMIT:?}), scroll frame {:?} per strip (limit {FRAME_LIMIT:?}), {} paths, {} vertices for {} columns in {} runs",
        m.build, m.frame, m.paths, m.vertices, m.columns, m.runs
    );
    assert!(m.paths <= MAX_BUCKETS + 1, "{} paths", m.paths);
    // Plan: at most 2 * (columns + 2 * buckets) vertices when runs are long; every run adds one
    // shared edge column, so the exact bound is in runs.
    assert!(
        m.vertices <= 4 * (m.columns + m.runs),
        "{} vertices",
        m.vertices
    );
    if !cfg!(debug_assertions) {
        assert!(m.build < BUILD_LIMIT * HEADROOM, "build {:?}", m.build);
        assert!(m.frame < FRAME_LIMIT * HEADROOM, "frame {:?}", m.frame);
    }
}

#[test]
#[ignore = "strict plan numbers; run by hand with --release -- --ignored --nocapture"]
fn the_waveform_meets_the_strict_budget() {
    let m = measure();
    println!("strict: build {:?}, frame {:?}", m.build, m.frame);
    assert!(m.build < BUILD_LIMIT, "build {:?}", m.build);
    assert!(m.frame < FRAME_LIMIT, "frame {:?}", m.frame);
}
