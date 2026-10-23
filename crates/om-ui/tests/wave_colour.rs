//! The spectrum waveform colour: synthetic tones order by hue, noise is pale, the overview and the
//! zoomed strip agree, and tracks use a wide range of hues (E10).

use om_analysis::{
    AudioClip, ClipSource, FileClipSource, NullAnalysisContext, WaveformBuilder, WaveformData,
};
use om_testing::mix::{mix_fixture, MixStyle};
use om_testing::signals::{noise, sine_amp};
use om_ui::theme::{parse_theme, ThemeTokens};
use om_ui::waveform::{
    hue_span, overview_column, overview_hues, strip_column, wave_colour, wave_hsl, Column,
    HUE_SPAN_COLUMNS,
};

const RATE: f64 = 44_100.0;

fn tokens() -> Vec<(&'static str, ThemeTokens)> {
    [
        (
            "dark",
            include_str!("../../../resources/themes/openmixer-dark.json"),
        ),
        (
            "daylight",
            include_str!("../../../resources/themes/openmixer-daylight.json"),
        ),
    ]
    .into_iter()
    .map(|(name, json)| (name, parse_theme(json).expect("built-in theme").tokens))
    .collect()
}

fn analyse(samples: Vec<f32>) -> WaveformData {
    let clip = AudioClip::new(samples, RATE);
    WaveformBuilder::default()
        .build(&clip, &NullAnalysisContext)
        .expect("not cancelled")
}

/// The column over the middle half of the track.
fn middle_column(data: &WaveformData) -> Column {
    let points = &data.levels()[0].points;
    let (from, to) = (points.len() / 4, points.len() * 3 / 4);
    strip_column(
        &data.levels()[0],
        (from * data.levels()[0].samples_per_point) as f64,
        ((to - from) * data.levels()[0].samples_per_point) as f64,
    )
    .expect("inside the track")
}

fn tone_column(hz: f64) -> Column {
    middle_column(&analyse(sine_amp(hz, 0.5, RATE, (6.0 * RATE) as usize)))
}

#[test]
fn tones_map_to_distinct_hues_in_order_of_frequency() {
    for (name, t) in tokens() {
        let hues: Vec<f32> = [60.0, 1000.0, 10_000.0]
            .iter()
            .map(|&hz| wave_hsl(&tone_column(hz), &t).hue)
            .collect();
        eprintln!("tone hues {name}: {hues:?}");
        assert!(hues[0] < hues[1] && hues[1] < hues[2], "{name}: {hues:?}");
        assert!(
            hues[1] - hues[0] > 60.0 && hues[2] - hues[1] > 60.0,
            "{name}: {hues:?}"
        );
    }
}

#[test]
fn white_noise_has_low_saturation_and_tones_do_not() {
    for (name, t) in tokens() {
        let noise_column = middle_column(&analyse(noise(11, (6.0 * RATE) as usize)));
        let pale = wave_hsl(&noise_column, &t).saturation;
        assert!(
            pale < 0.3,
            "{name}: noise saturation {pale} for {noise_column:?}"
        );
        for hz in [60.0, 1000.0] {
            let vivid = wave_hsl(&tone_column(hz), &t).saturation;
            assert!(vivid > 0.7, "{name}: {hz} Hz saturation {vivid}");
        }
    }
}

#[test]
fn a_broad_spectrum_is_less_saturated_than_a_narrow_one_at_the_same_pitch() {
    let t = ThemeTokens::builtin_dark();
    let at = |spread: f32| {
        let column = Column {
            loudness: 0.3,
            centroid: 0.5,
            spread,
            peak: 1.0,
        };
        wave_hsl(&column, &t).saturation
    };
    let octaves = |o: f32| o / om_analysis::SPREAD_MAX_OCTAVES;
    assert!(at(octaves(0.3)) > at(octaves(2.0)) && at(octaves(2.0)) > at(octaves(3.8)));
    assert!(at(octaves(3.8)) > 0.2, "broad music stays coloured");
}

#[test]
fn louder_is_brighter_and_silence_is_dimmest() {
    let t = ThemeTokens::builtin_dark();
    let at = |loudness: f32| {
        wave_hsl(
            &Column {
                loudness,
                centroid: 0.5,
                ..Column::default()
            },
            &t,
        )
        .lightness
    };
    assert!(at(0.001) < at(0.02) && at(0.02) < at(0.1) && at(0.1) < at(0.5));
    assert!((at(0.0) - t.wave_lightness_min).abs() < 1e-6);
    assert!(at(1.0) <= t.wave_lightness_max);
}

#[test]
fn colours_stay_legible_in_both_themes() {
    for (name, t) in tokens() {
        for i in 0..=100 {
            for loudness in [0.0, 0.01, 0.1, 1.0] {
                let c = Column {
                    loudness,
                    centroid: i as f32 / 100.0,
                    spread: 0.1,
                    peak: 1.0,
                };
                let hsl = wave_hsl(&c, &t);
                assert!(hsl.saturation <= t.wave_saturation_max + 1e-6, "{name}");
                assert!(hsl.lightness >= t.wave_lightness_min - 1e-6, "{name}");
                assert!(hsl.lightness <= t.wave_lightness_max + 1e-6, "{name}");
            }
        }
    }
}

#[test]
fn a_waveform_without_spectrum_draws_neutral_grey() {
    let t = ThemeTokens::builtin_dark();
    let loud_high = Column {
        loudness: 0.5,
        centroid: 0.8,
        spread: 0.1,
        peak: 1.0,
    };
    let quiet_low = Column {
        loudness: 0.05,
        centroid: 0.1,
        spread: 0.1,
        peak: 0.2,
    };
    let (a, b) = (
        wave_colour(&loud_high, false, &t),
        wave_colour(&quiet_low, false, &t),
    );
    assert_eq!(a, b, "no hue and no loudness shading without a spectrum");
    let rgb = a.argb();
    assert_eq!((rgb >> 16) & 0xFF, rgb & 0xFF, "grey");
    assert_ne!(wave_colour(&loud_high, true, &t), a);
}

#[test]
fn overview_and_zoomed_strip_give_the_same_hue_for_the_same_section() {
    let t = ThemeTokens::builtin_dark();
    let data = analyse(mix_fixture(MixStyle::Full, RATE, 96.0));
    let fine = &data.levels()[0];
    let points = &data.overview().points;
    let columns = 240;
    let per_point = data.overview().samples_per_point as f64;
    for i in (0..columns - 1).step_by(7) {
        let overview = overview_column(points, i, columns);
        // The span of overview points this column folds, and the same span in fine points.
        let lo = i * points.len() / columns;
        let hi = ((i + 1) * points.len() / columns).max(lo + 1);
        let zoomed = strip_column(fine, lo as f64 * per_point, (hi - lo) as f64 * per_point)
            .expect("in range");
        let (a, b) = (wave_hsl(&overview, &t), wave_hsl(&zoomed, &t));
        let hue_gap = (a.hue - b.hue).abs().min(360.0 - (a.hue - b.hue).abs());
        assert!(
            hue_gap < 4.0,
            "column {i}: overview {} zoomed {}",
            a.hue,
            b.hue
        );
        assert!((a.lightness - b.lightness).abs() < 0.04, "column {i}");
    }
}

/// Hue range guard, locked from the first measurement (see the E10 PR): the synthetic full mix
/// spans 212 degrees, the vocal-led mix 194, and "Make some room" by Renaux 201 (dark) and 199
/// (daylight). The limit sits well below them: it catches a collapse to a few hues, not taste.
const MIN_HUE_SPAN_DEGREES: f32 = 150.0;
/// A sparse pad-and-hats mix measured 57 degrees; it only has to stay clearly above one hue.
const MIN_SPARSE_HUE_SPAN_DEGREES: f32 = 30.0;

fn span_of(data: &WaveformData, t: &ThemeTokens) -> f32 {
    hue_span(&overview_hues(data, HUE_SPAN_COLUMNS, t))
}

#[test]
fn the_synthetic_mixes_use_a_wide_range_of_hues() {
    for (name, t) in tokens() {
        for (style, floor) in [
            (MixStyle::Full, MIN_HUE_SPAN_DEGREES),
            (MixStyle::VocalLed, MIN_HUE_SPAN_DEGREES),
            (MixStyle::Sparse, MIN_SPARSE_HUE_SPAN_DEGREES),
        ] {
            let span = span_of(&analyse(mix_fixture(style, RATE, 96.0)), &t);
            eprintln!("hue span {name} {style:?}: {span:.0} degrees (limit {floor})");
            assert!(span >= floor, "{name} {style:?}: {span}");
        }
    }
}

#[test]
fn a_bass_only_mix_stays_in_the_warm_end() {
    for (name, t) in tokens() {
        let data = analyse(mix_fixture(MixStyle::HeavyBass, RATE, 96.0));
        let mut hues = overview_hues(&data, HUE_SPAN_COLUMNS, &t);
        assert!(!hues.is_empty());
        // The quiet hats-only intro can wash toward white; the bulk of the track is warm.
        hues.sort_by(f32::total_cmp);
        let median = hues[hues.len() / 2];
        assert!(
            !(60.0..=340.0).contains(&median),
            "{name}: median hue {median}"
        );
        assert!(
            hue_span(&hues) < 60.0,
            "{name}: one warm group, not a rainbow"
        );
    }
}

/// Reference tracks are never committed: set `OPENMIXER_REFERENCE_TRACKS` to a folder or a
/// `:`-separated list of files to run this on real audio. Without it the test does nothing.
#[test]
fn reference_tracks_use_a_wide_range_of_hues() {
    let Ok(spec) = std::env::var("OPENMIXER_REFERENCE_TRACKS") else {
        eprintln!("OPENMIXER_REFERENCE_TRACKS not set: no reference tracks checked");
        return;
    };
    let tracks = reference_files(&spec);
    assert!(!tracks.is_empty(), "no audio files in {spec}");
    for path in tracks {
        let clip = FileClipSource::new(&path)
            .load(&NullAnalysisContext)
            .expect("decodes");
        let data = WaveformBuilder::default()
            .build(&clip, &NullAnalysisContext)
            .expect("not cancelled");
        for (name, t) in tokens() {
            let span = span_of(&data, &t);
            eprintln!("hue span {name} {}: {span:.0} degrees", path.display());
            assert!(
                span >= MIN_HUE_SPAN_DEGREES,
                "{name} {}: {span}",
                path.display()
            );
        }
    }
}

fn reference_files(spec: &str) -> Vec<std::path::PathBuf> {
    let audio = |p: &std::path::Path| {
        matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("wav" | "flac" | "mp3" | "ogg" | "m4a")
        )
    };
    let mut files = Vec::new();
    for part in spec.split(':').filter(|p| !p.is_empty()) {
        let path = std::path::PathBuf::from(part);
        match std::fs::read_dir(&path) {
            Ok(entries) => files.extend(entries.flatten().map(|e| e.path()).filter(|p| audio(p))),
            Err(_) => files.push(path),
        }
    }
    files.sort();
    files
}

/// Painting cost: four decks, each a zoomed strip and an overview wide, so the colour work of one
/// frame at 1920 px. The frame budget is 16.7 ms at 60 Hz; the colouring must stay a small part.
#[test]
fn colouring_a_frame_of_four_decks_is_cheap() {
    let t = ThemeTokens::builtin_dark();
    let data = analyse(mix_fixture(MixStyle::Full, RATE, 300.0));
    let (width, decks) = (1920usize, 4);
    let started = std::time::Instant::now();
    let mut checksum = 0u32;
    for deck in 0..decks {
        let level = data.level_for(256.0);
        for x in 0..width {
            let frame = (deck * 100_000 + x * 256) as f64;
            if let Some(c) = strip_column(level, frame, 256.0) {
                checksum ^= wave_colour(&c, true, &t).argb();
            }
            let o = overview_column(&data.overview().points, x, width);
            checksum ^= wave_colour(&o, true, &t).argb();
        }
    }
    let elapsed = started.elapsed();
    eprintln!(
        "colouring {} columns: {elapsed:?} (checksum {checksum})",
        width * decks * 2
    );
    // Measured about 1 ms in the unoptimised test build; 8 ms is half a 60 Hz frame.
    assert!(elapsed.as_secs_f64() < 0.008, "{elapsed:?}");
}
