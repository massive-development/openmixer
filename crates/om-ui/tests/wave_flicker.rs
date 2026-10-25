//! Flicker gates for the waveform colour and height. A strip column must be a pure function of its
//! absolute position in the track (not of the screen column or the scroll offset): Mixxx snaps the
//! first drawn frame to a whole multiple of the per-pixel increment for the same reason
//! (`qRound(firstVisualFrame / visualIncrementPerPixel) * visualIncrementPerPixel` in
//! allshader/waveformrendererrgb.cpp) and takes each pixel's maximum over a window of one
//! increment centred on it. The gates, defined before the fix and measured on the demo loop here
//! and on a real track by hand (`TRACK=path cargo test --release -p om-ui --test wave_flicker --
//! --ignored --nocapture`):
//!
//! (a) spatial: median and 95th percentile of the colour difference between adjacent drawn
//!     columns in steady sections (printed, for smoothing radius 0, 1 and 2);
//! (b) temporal: the same absolute columns get the same shape whatever the scroll offset
//!     (asserted);
//! (c) seams: the shapes of consecutive runs abut with no gap (asserted).

use om_analysis::{
    demo_loop, AudioClip, ClipSource, FileClipSource, NullAnalysisContext, WaveformBuilder,
    WaveformData,
};
use om_ui::theme::{Colour, ThemeTokens};
use om_ui::waveform::{
    column_shapes, column_tiles, reference_peak, strip_column, strip_well, BandStrip, Column,
    ColumnShape, Palette, StripSource, View, WaveStyle, COLOUR_SMOOTH_RADIUS, STRIP_STEP_PX,
};

const RATE: f64 = 44_100.0;
const WIDTH: f64 = 1280.0;
/// Columns at each end of a cached strip left out of the comparison.
const EDGE: usize = COLOUR_SMOOTH_RADIUS + 1;

fn demo() -> (WaveformData, i64) {
    let clip = AudioClip::new(demo_loop(64.0, 124.0, RATE), RATE);
    let length = clip.samples.len() as i64;
    let data = WaveformBuilder::default()
        .build(&clip, &NullAnalysisContext)
        .expect("not cancelled");
    (data, length)
}

fn view(centre: f64) -> View {
    View {
        centre_frame: centre.round() as i64,
        frames_per_pixel: 8.0 * RATE / WIDTH,
        width: WIDTH,
    }
}

fn strip(data: &WaveformData, length: i64, v: &View) -> BandStrip {
    let level = data.level_for(v.frames_per_pixel);
    let src = StripSource {
        track: 1,
        level,
        height_px: 96,
        length_frames: length,
        reference: reference_peak(&data.overview().points),
        spectral: true,
    };
    BandStrip::render(&src, v)
}

/// The absolute column index of every interior column of `s`, with its shape.
fn absolute(s: &BandStrip) -> Vec<(i64, ColumnShape)> {
    let n = s.columns.len();
    (EDGE..n - EDGE)
        .map(|i| {
            let frame = s.first_frame + i as f64 * s.step_frames;
            ((frame / s.step_frames).round() as i64, s.columns[i])
        })
        .collect()
}

/// (shared, differing) interior columns of two strips, matched by absolute position.
fn mismatch(a: &BandStrip, b: &BandStrip) -> (usize, usize) {
    let (a, b) = (absolute(a), absolute(b));
    let mut shared = 0;
    let mut differ = 0;
    for (index, shape) in &a {
        if let Some((_, other)) = b.iter().find(|(j, _)| j == index) {
            shared += 1;
            differ += usize::from(shape != other);
        }
    }
    (shared, differ)
}

#[test]
fn colour_and_height_are_a_function_of_absolute_position() {
    let (data, length) = demo();
    let centre = length as f64 / 2.0;
    let fpp = 8.0 * RATE / WIDTH;
    let base = strip(&data, length, &view(centre));
    // A third of a pixel, one pixel, odd amounts, and a whole view of scroll.
    for offset_px in [0.3, 1.0, 1.7, 5.0, 123.4, WIDTH] {
        let moved = strip(&data, length, &view(centre + offset_px * fpp));
        let (shared, differ) = mismatch(&base, &moved);
        println!("scroll {offset_px} px: {differ} of {shared} shared columns differ");
        assert!(shared > 100, "strips share columns at {offset_px} px");
        assert_eq!(
            differ, 0,
            "{differ} of {shared} columns changed at {offset_px} px"
        );
    }
}

#[test]
fn drawn_tiles_leave_no_seam_while_scrolling() {
    let (data, length) = demo();
    let fpp = 8.0 * RATE / WIDTH;
    let centre = length as f64 / 2.0;
    for offset_px in [0.0, 0.2, 0.5, 0.8, 1.3] {
        let v = view(centre + offset_px * fpp);
        let s = strip(&data, length, &v);
        let centres: Vec<f32> = s.visible(&v).iter().map(|(x, _)| *x).collect();
        let tiles = column_tiles(&centres);
        for pair in tiles.windows(2) {
            assert_eq!(pair[0].0 + pair[0].1, pair[1].0, "seam at {offset_px} px");
        }
    }
}

fn rgb(c: Colour) -> [f32; 3] {
    [16, 8, 0].map(|s| ((c.argb() >> s) & 0xFF) as f32 / 255.0)
}

fn distance(a: Colour, b: Colour) -> f32 {
    let (a, b) = (rgb(a), rgb(b));
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

fn percentile(v: &mut [f32], p: usize) -> f32 {
    v.sort_by(f32::total_cmp);
    v.get(v.len().saturating_sub(1) * p / 100)
        .copied()
        .unwrap_or(0.0)
}

/// (median, p95) of the colour difference between adjacent columns in steady sections (both
/// columns tall, heights within 0.05), over a 2000 column stretch from the middle of the track.
fn adjacent(data: &WaveformData, length: i64) -> ((f32, f32), usize) {
    let points = &data.overview().points;
    let reference = reference_peak(points);
    let palette = Palette::new(WaveStyle::Colour, strip_well(&ThemeTokens::builtin_dark()));
    let fpp = 8.0 * RATE / WIDTH;
    let step = STRIP_STEP_PX * fpp;
    let level = data.level_for(fpp);
    let first = (length as f64 / 2.0 / step).floor() * step;
    let columns: Vec<Option<Column>> = (0..2000)
        .map(|i| strip_column(level, first + i as f64 * step, step))
        .collect();
    let shapes = column_shapes(&columns, reference, true);
    let mut drawn = Vec::new();
    for pair in shapes.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.silhouette < 0.3 || b.silhouette < 0.3 || (a.silhouette - b.silhouette).abs() > 0.05 {
            continue;
        }
        drawn.push(distance(
            palette.colour(a.weights),
            palette.colour(b.weights),
        ));
    }
    let n = drawn.len();
    ((percentile(&mut drawn, 50), percentile(&mut drawn, 95)), n)
}

/// What averaging the band weights over `radius` columns either side would do: the p95 step
/// between adjacent columns, and the share of columns whose colour moves by more than 0.3 (detail
/// that a smoothing would hide).
fn smoothing_effect(data: &WaveformData, length: i64, radius: usize) -> (f32, f32) {
    let reference = reference_peak(&data.overview().points);
    let palette = Palette::new(WaveStyle::Colour, strip_well(&ThemeTokens::builtin_dark()));
    let fpp = 8.0 * RATE / WIDTH;
    let step = STRIP_STEP_PX * fpp;
    let level = data.level_for(fpp);
    let first = (length as f64 / 2.0 / step).floor() * step;
    let columns: Vec<Option<Column>> = (0..2000)
        .map(|i| strip_column(level, first + i as f64 * step, step))
        .collect();
    let shapes = column_shapes(&columns, reference, true);
    let smooth: Vec<[f32; 3]> = (0..shapes.len())
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(radius), (i + radius + 1).min(shapes.len()));
            let mut m = [0.0f32; 3];
            for s in &shapes[lo..hi] {
                for (a, b) in m.iter_mut().zip(s.weights) {
                    *a += b / (hi - lo) as f32;
                }
            }
            m
        })
        .collect();
    let (mut steps, mut moved, mut n) = (Vec::new(), 0, 0);
    for i in 0..shapes.len() - 1 {
        if shapes[i].silhouette < 0.3 {
            continue;
        }
        n += 1;
        moved += usize::from(
            distance(palette.colour(shapes[i].weights), palette.colour(smooth[i])) > 0.3,
        );
        if shapes[i + 1].silhouette >= 0.3
            && (shapes[i].silhouette - shapes[i + 1].silhouette).abs() <= 0.05
        {
            steps.push(distance(
                palette.colour(smooth[i]),
                palette.colour(smooth[i + 1]),
            ));
        }
    }
    (percentile(&mut steps, 95), moved as f32 / n.max(1) as f32)
}

fn print_adjacent(label: &str, data: &WaveformData, length: i64) {
    for radius in [1, 2, 4] {
        let (p95, moved) = smoothing_effect(data, length, radius);
        println!("{label}: if smoothed over {radius} columns either side: p95 step {p95:.3}, {:.1}% of columns change colour by > 0.3", 100.0 * moved);
    }
    let (d, n) = adjacent(data, length);
    println!(
        "{label}, {n} steady pairs: colour difference between adjacent columns median {:.3} p95 {:.3}",
        d.0, d.1
    );
}

#[test]
fn adjacent_columns_on_the_demo_loop() {
    let (data, length) = demo();
    print_adjacent("demo loop", &data, length);
}

/// By hand on a real file.
#[test]
#[ignore = "needs TRACK=path to an audio file"]
fn adjacent_columns_and_height_reference_on_a_real_track() {
    let path = std::env::var("TRACK").expect("TRACK");
    let clip = FileClipSource::new(std::path::Path::new(&path))
        .load(&NullAnalysisContext)
        .expect("decodes");
    let length = clip.samples.len() as i64;
    let data = WaveformBuilder::default()
        .build(&clip, &NullAnalysisContext)
        .unwrap();
    print_adjacent("real track", &data, length);
    let points = &data.overview().points;
    let mut sorted: Vec<f32> = points.iter().map(|p| p.peak()).collect();
    sorted.sort_by(f32::total_cmp);
    for percentile in [95.0f32, 99.0, 99.9, 100.0] {
        let rank = ((percentile / 100.0) * (sorted.len() - 1) as f32).round() as usize;
        let reference = sorted[rank].max(0.1);
        let columns = 1600;
        let mut heights: Vec<f32> = (0..columns)
            .map(|i| {
                let c = om_ui::waveform::overview_column(points, i, columns);
                (c.peak / reference).clamp(0.0, 1.0).powf(0.6)
            })
            .collect();
        let full = heights.iter().filter(|&&h| h >= 1.0).count() as f32 / columns as f32;
        let near = heights.iter().filter(|&&h| h >= 0.95).count() as f32 / columns as f32;
        heights.sort_by(f32::total_cmp);
        let spread = heights[columns * 95 / 100] / heights[columns * 20 / 100].max(1e-3);
        println!(
            "reference at {percentile}th percentile ({reference:.3}): full height {:.1}%, >= 0.95 {:.1}%, p95/p20 height {:.2}",
            100.0 * full,
            100.0 * near,
            spread
        );
    }
    // Alternative: height from the RMS (loudness) against its own 95th percentile, the usual
    // answer for a limited master whose peaks all sit at full scale.
    let mut loud: Vec<f32> = points.iter().map(|p| p.loudness).collect();
    loud.sort_by(f32::total_cmp);
    let reference = loud[(loud.len() - 1) * 95 / 100].max(0.05);
    let columns = 1600;
    let mut heights: Vec<f32> = (0..columns)
        .map(|i| {
            let c = om_ui::waveform::overview_column(points, i, columns);
            (c.loudness / reference).clamp(0.0, 1.0).powf(0.6)
        })
        .collect();
    let full = heights.iter().filter(|&&h| h >= 1.0).count() as f32 / columns as f32;
    heights.sort_by(f32::total_cmp);
    let spread = heights[columns * 95 / 100] / heights[columns * 20 / 100].max(1e-3);
    println!(
        "RMS height against its 95th percentile ({reference:.3}): full height {:.1}%, p95/p20 height {:.2}",
        100.0 * full,
        spread
    );
}
