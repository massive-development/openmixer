//! What sits on top of the waveform: playhead, beat and bar lines, hot cue flags, the loop region
//! and the overview's markers. The sizes, colours and the geometry that decides what is shown
//! live here, pure, so they are tested; the GPUI layer only paints what these return. The values
//! are local to the waveform and do not come from the theme.

use super::{frame_to_x, View};
use crate::theme::Colour;
use om_engine::grid::BeatGrid;

// Playhead: opaque, 2 px, with a 1 px dark halo each side so it reads on any colour.
pub const PLAYHEAD_WIDTH: f32 = 2.0;
pub const PLAYHEAD_HALO: f32 = 1.0;
pub const PLAYHEAD_COLOUR: Colour = Colour::from_argb(0xFFFF_FFFF);
pub const PLAYHEAD_HALO_COLOUR: Colour = Colour::from_argb(0xFF0B_0D12);

// Beat grid.
pub const BEAT_LINE_WIDTH: f32 = 1.0;
pub const BEAT_LINE_ALPHA: f32 = 0.25;
pub const BAR_LINE_WIDTH: f32 = 2.0;
pub const BAR_LINE_ALPHA: f32 = 0.70;
pub const GRID_COLOUR: Colour = Colour::from_argb(0xFFFF_FFFF);
/// Bar numbers are at least this tall, in px.
pub const BAR_NUMBER_SIZE: f32 = 12.0;
/// A bar number needs this much room to its right, so numbers never touch.
pub const BAR_NUMBER_MIN_SPACING_PX: f32 = 48.0;

// Cue flags.
pub const CUE_FLAG_WIDTH: f32 = 18.0;
pub const CUE_FLAG_HEIGHT: f32 = 16.0;
pub const CUE_LINE_WIDTH: f32 = 2.0;
pub const CUE_NUMBER_SIZE: f32 = 12.0;
/// The main cue point (not one of the numbered hot cues).
pub const MAIN_CUE_COLOUR: Colour = Colour::from_argb(0xFFFF_6B35);
/// Hot cue 1..=8 colours; further cues repeat the cycle. Chosen to stay apart from the three
/// band colours (blue, orange, white) and from the loop green.
pub const CUE_PALETTE: [Colour; 8] = [
    Colour::from_argb(0xFFFF_4D6D),
    Colour::from_argb(0xFFFF_D23F),
    Colour::from_argb(0xFF4D_D964),
    Colour::from_argb(0xFF22_D3EE),
    Colour::from_argb(0xFF8B_7BFF),
    Colour::from_argb(0xFFFF_6EC7),
    Colour::from_argb(0xFFB6_E83A),
    Colour::from_argb(0xFF3D_8BFF),
];
/// Text on a cue flag.
pub const CUE_TEXT_COLOUR: Colour = Colour::from_argb(0xFF0B_0D12);

// Loop region.
pub const LOOP_COLOUR: Colour = Colour::from_argb(0xFF2E_E59D);
pub const LOOP_FILL_ALPHA: f32 = 0.18;
/// A loop that is set but not engaged is dimmer.
pub const LOOP_IDLE_FILL_ALPHA: f32 = 0.07;
pub const LOOP_EDGE_WIDTH: f32 = 2.0;
pub const LOOP_EDGE_ALPHA: f32 = 1.0;
pub const LOOP_IDLE_EDGE_ALPHA: f32 = 0.45;

// Overview.
/// Height of an overview bar tick, in px.
pub const OVERVIEW_TICK_HEIGHT: f32 = 4.0;
pub const OVERVIEW_TICK_ALPHA: f32 = 0.35;
/// Bar ticks on the overview are thinned until they are at least this far apart.
pub const OVERVIEW_TICK_MIN_SPACING_PX: f32 = 8.0;
pub const OVERVIEW_CUE_TICK_WIDTH: f32 = 2.0;

/// Colour of hot cue `index` (0-based).
pub fn cue_colour(index: usize) -> Colour {
    CUE_PALETTE[index % CUE_PALETTE.len()]
}

/// One cue flag in the zoomed strip: the line at `x` and the tab to its right.
#[derive(Debug, Clone, PartialEq)]
pub struct CueFlag {
    /// 0-based hot cue index.
    pub index: usize,
    pub x: f32,
    pub colour: Colour,
    /// The number shown on the tab (1-based).
    pub label: String,
}

/// Flags for the cues whose line or tab touches a strip `width` px wide. Unset cues are `None`.
pub fn cue_flags(xs: &[Option<f64>], width: f64) -> Vec<CueFlag> {
    xs.iter()
        .enumerate()
        .filter_map(|(index, x)| {
            let x = (*x)?;
            let on_screen =
                x + f64::from(CUE_FLAG_WIDTH) >= 0.0 && x - f64::from(CUE_LINE_WIDTH) <= width;
            on_screen.then(|| CueFlag {
                index,
                x: x as f32,
                colour: cue_colour(index),
                label: (index + 1).to_string(),
            })
        })
        .collect()
}

/// The loop in the zoomed strip.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopRegion {
    pub x0: f64,
    pub x1: f64,
    /// Engaged (looping now) as opposed to set but idle.
    pub looping: bool,
    /// Length in beats, e.g. `4` or `1/2`; empty without a beat grid.
    pub label: String,
}

impl LoopRegion {
    pub fn fill_alpha(&self) -> f32 {
        if self.looping {
            LOOP_FILL_ALPHA
        } else {
            LOOP_IDLE_FILL_ALPHA
        }
    }

    pub fn edge_alpha(&self) -> f32 {
        if self.looping {
            LOOP_EDGE_ALPHA
        } else {
            LOOP_IDLE_EDGE_ALPHA
        }
    }
}

/// A loop length in beats as the loop buttons name it: whole numbers, or `1/2`, `1/4`, `1/8`,
/// `1/16`, `1/32` for the short ones. Anything else is shown to one decimal. Empty when the
/// length is not positive.
pub fn loop_label(beats: f64) -> String {
    if !beats.is_finite() || beats <= 0.0 {
        return String::new();
    }
    if beats >= 1.0 && (beats - beats.round()).abs() < 0.02 {
        return format!("{}", beats.round() as i64);
    }
    for denominator in [2, 4, 8, 16, 32] {
        if (beats - 1.0 / f64::from(denominator)).abs() < 0.01 / f64::from(denominator) {
            return format!("1/{denominator}");
        }
    }
    format!("{beats:.1}")
}

/// Loop length in beats under `grid`.
pub fn loop_beats(grid: &dyn BeatGrid, start: i64, end: i64) -> f64 {
    grid.beat_at_frame(end) - grid.beat_at_frame(start)
}

/// Bar number printed at a bar line: bars count from 1 at the grid origin. `None` before it.
pub fn bar_number(beat: f64, beats_per_bar: i64) -> Option<i64> {
    let bar = (beat.round() as i64).div_euclid(beats_per_bar.max(1)) + 1;
    (bar >= 1).then_some(bar)
}

/// Prints a number only on every n-th bar (a power of two) so labels keep at least
/// [`BAR_NUMBER_MIN_SPACING_PX`] between them. `bar_px` is the width of one bar.
pub fn bar_label_step(bar_px: f32) -> i64 {
    let bar_px = bar_px.abs().max(f32::EPSILON);
    let mut step = 1;
    while bar_px * (step as f32) < BAR_NUMBER_MIN_SPACING_PX && step < (1 << 20) {
        step *= 2;
    }
    step
}

/// The overview's markers, as x positions across its width.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OverviewMarkers {
    pub playhead_x: f64,
    /// Set hot cues: x and colour.
    pub cues: Vec<(f64, Colour)>,
    /// The loop: x of both ends, and whether it is engaged.
    pub loop_span: Option<(f64, f64, bool)>,
    /// Bar ticks, thinned to stay at least [`OVERVIEW_TICK_MIN_SPACING_PX`] apart.
    pub bar_ticks: Vec<f64>,
}

/// Overview x of bar lines under `grid` over a track of `length` frames drawn `width` px wide. Bars
/// are thinned by powers of two until they are far enough apart.
pub fn overview_bar_ticks(grid: &dyn BeatGrid, length: i64, width: f64) -> Vec<f64> {
    if length <= 0 || width <= 0.0 {
        return Vec::new();
    }
    let per_bar = i64::from(grid.beats_per_bar().max(1));
    let bar_px = frame_to_x(
        grid.frame_at_beat(per_bar as f64) - grid.frame_at_beat(0.0),
        length,
        width,
    );
    if bar_px <= 0.0 {
        return Vec::new();
    }
    let mut step = 1_i64;
    while bar_px * (step as f64) < f64::from(OVERVIEW_TICK_MIN_SPACING_PX) && step < (1 << 20) {
        step *= 2;
    }
    let last_beat = grid.beat_at_frame(length);
    let first_bar = (grid.beat_at_frame(0).max(0.0) / per_bar as f64).ceil() as i64;
    let first_bar = (first_bar + step - 1).div_euclid(step) * step;
    let mut ticks = Vec::new();
    let mut bar = first_bar;
    while (bar * per_bar) as f64 <= last_beat {
        let frame = grid.frame_at_beat((bar * per_bar) as f64);
        if (0..length).contains(&frame) {
            ticks.push(frame_to_x(frame, length, width));
        }
        bar += step;
    }
    ticks
}

/// Alpha-applied grid colour for a line.
pub fn grid_line_colour(bar: bool) -> Colour {
    GRID_COLOUR.with_alpha(if bar { BAR_LINE_ALPHA } else { BEAT_LINE_ALPHA })
}

/// The playhead's x range: the 2 px bar and its halo, as (left of halo, total width). Centred on
/// `x`, snapped to whole pixels so the line does not shimmer.
pub fn playhead_rect(x: f64) -> (f32, f32) {
    let width = PLAYHEAD_WIDTH + 2.0 * PLAYHEAD_HALO;
    ((x.round() as f32) - width / 2.0, width)
}

/// Where the already-played part of a zoomed strip ends: the playhead x, clamped into the view.
pub fn played_edge(view: &View, playhead_x: f64) -> f64 {
    playhead_x.clamp(0.0, view.width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_engine::grid::FixedTempoGrid;

    fn grid(bpm: f64) -> FixedTempoGrid {
        FixedTempoGrid::new(bpm, 44_100.0, 0)
    }

    #[test]
    fn cue_palette_has_eight_distinct_opaque_colours_apart_from_the_main_cue() {
        for (i, a) in CUE_PALETTE.iter().enumerate() {
            assert_eq!(a.alpha(), 1.0);
            assert_ne!(*a, MAIN_CUE_COLOUR);
            for b in &CUE_PALETTE[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(MAIN_CUE_COLOUR.argb(), 0xFFFF_6B35);
        assert_eq!(cue_colour(0), CUE_PALETTE[0]);
        assert_eq!(cue_colour(8), CUE_PALETTE[0], "cycles");
    }

    #[test]
    fn flags_are_numbered_from_one_and_skip_unset_or_offscreen_cues() {
        let xs = [Some(100.0), None, Some(-500.0), Some(-10.0), Some(900.0)];
        let flags = cue_flags(&xs, 800.0);
        let labels: Vec<_> = flags.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["1", "4"], "cue 3 is far left, 5 is off the right");
        assert_eq!(flags[0].colour, cue_colour(0));
        assert_eq!(flags[1].index, 3);
        assert!(cue_flags(&[None; 8], 800.0).is_empty());
    }

    #[test]
    fn loop_lengths_read_like_the_loop_buttons() {
        assert_eq!(loop_label(4.0), "4");
        assert_eq!(loop_label(16.0), "16");
        assert_eq!(loop_label(1.0), "1");
        assert_eq!(loop_label(0.5), "1/2");
        assert_eq!(loop_label(0.25), "1/4");
        assert_eq!(loop_label(1.0 / 32.0), "1/32");
        assert_eq!(loop_label(2.004), "2", "grid rounding noise");
        assert_eq!(loop_label(0.0), "");
        assert_eq!(loop_label(f64::NAN), "");
        assert_eq!(loop_label(2.5), "2.5");
    }

    #[test]
    fn loop_beats_come_from_the_grid() {
        let g = grid(120.0); // 22050 frames per beat
        assert!((loop_beats(&g, 22_050, 22_050 * 5) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn an_idle_loop_is_dimmer_than_an_engaged_one() {
        let on = LoopRegion {
            x0: 0.0,
            x1: 10.0,
            looping: true,
            label: "4".into(),
        };
        let off = LoopRegion {
            looping: false,
            ..on.clone()
        };
        assert_eq!(on.fill_alpha(), 0.18);
        assert!(off.fill_alpha() < on.fill_alpha());
        assert!(off.edge_alpha() < on.edge_alpha());
        assert_eq!(LOOP_COLOUR.argb(), 0xFF2E_E59D);
    }

    #[test]
    fn bars_count_from_one_at_the_grid_origin() {
        assert_eq!(bar_number(0.0, 4), Some(1));
        assert_eq!(bar_number(4.0, 4), Some(2));
        assert_eq!(bar_number(36.0, 4), Some(10));
        assert_eq!(bar_number(-4.0, 4), None, "before the grid");
        assert_eq!(
            bar_number(8.0, 0),
            Some(9),
            "a zero bar length is treated as one"
        );
    }

    #[test]
    fn bar_labels_thin_out_by_powers_of_two_to_keep_their_room() {
        assert_eq!(bar_label_step(200.0), 1);
        assert_eq!(bar_label_step(48.0), 1);
        assert_eq!(bar_label_step(40.0), 2);
        assert_eq!(bar_label_step(20.0), 4);
        assert_eq!(bar_label_step(11.0), 8);
        for px in [3.0, 9.0, 17.0, 31.0, 47.0, 90.0] {
            assert!(bar_label_step(px) as f32 * px >= BAR_NUMBER_MIN_SPACING_PX || px >= 48.0);
        }
    }

    #[test]
    fn grid_lines_are_faint_beats_and_firm_bars() {
        assert!((grid_line_colour(false).alpha() - 0.25).abs() < 0.01);
        assert!((grid_line_colour(true).alpha() - 0.70).abs() < 0.01);
        assert_eq!((BEAT_LINE_WIDTH, BAR_LINE_WIDTH), (1.0, 2.0));
    }

    #[test]
    fn playhead_is_two_opaque_pixels_with_a_halo() {
        assert_eq!(PLAYHEAD_WIDTH, 2.0);
        assert_eq!(PLAYHEAD_COLOUR.alpha(), 1.0);
        assert_eq!(PLAYHEAD_HALO_COLOUR.alpha(), 1.0);
        let (left, width) = playhead_rect(200.0);
        assert_eq!(width, 4.0);
        assert_eq!(left, 198.0, "bar spans 199..201 inside a 1 px halo");
        assert_eq!(
            playhead_rect(200.4),
            playhead_rect(200.0),
            "snaps to pixels"
        );
    }

    #[test]
    fn played_edge_stays_inside_the_view() {
        let v = View {
            centre_frame: 0,
            frames_per_pixel: 1.0,
            width: 400.0,
        };
        assert_eq!(played_edge(&v, 200.0), 200.0);
        assert_eq!(played_edge(&v, -50.0), 0.0);
        assert_eq!(played_edge(&v, 900.0), 400.0);
    }

    #[test]
    fn overview_ticks_follow_bars_and_thin_out_when_dense() {
        let g = grid(120.0); // a bar is 88_200 frames
        let length = 88_200 * 64; // 64 bars
        let wide = overview_bar_ticks(&g, length, 640.0); // 10 px per bar
        assert_eq!(wide.len(), 64, "{wide:?}");
        assert!((wide[1] - wide[0] - 10.0).abs() < 0.01);
        let narrow = overview_bar_ticks(&g, length, 320.0); // 5 px per bar -> every 2nd
        assert_eq!(narrow.len(), 32);
        assert!(narrow.windows(2).all(|w| w[1] - w[0] >= 8.0 - 1e-6));
        assert!(overview_bar_ticks(&g, 0, 640.0).is_empty());
        assert!(overview_bar_ticks(&g, length, 0.0).is_empty());
    }
}
