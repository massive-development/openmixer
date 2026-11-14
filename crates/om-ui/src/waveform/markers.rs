//! What sits on top of the waveform: playhead, beat and bar lines, hot cue flags, the loop region
//! and the overview's markers. The sizes and the geometry that decides what is shown live here,
//! pure, so they are tested; the GPUI layer only paints what these return. Colours and strengths
//! come from the theme through [`StripInk`] (comp 4a: playhead in signal red, cue flags and loops
//! in the deck colour, the grid in the text colour).

use super::palette::{ensure_contrast, strip_well, Palette, WaveStyle, LINE_CONTRAST};
use super::{frame_to_x, View};
use crate::style::contrast_ratio;
use crate::theme::{Colour, ThemeTokens};
use om_engine::grid::BeatGrid;

/// The earlier styles keep a 1 px halo in the well colour each side of the playhead, so the red
/// line reads over their red band.
pub const PLAYHEAD_HALO: f32 = 1.0;

// Beat grid: 1 px lines (comp 4a); strengths are the theme's `gridBarAlpha` and `gridBeatAlpha`.
pub const BEAT_LINE_WIDTH: f32 = 1.0;
pub const BAR_LINE_WIDTH: f32 = 1.0;
/// The grid and bar number colour of the earlier styles, on their always-dark [`super::WAVE_WELL`].
pub const GRID_COLOUR: Colour = Colour::from_argb(0xFFFF_FFFF);
/// Alpha of the earlier styles' bar numbers in [`GRID_COLOUR`].
pub const LEGACY_NUMBER_ALPHA: f32 = 0.85;
/// Bar numbers are at least this tall, in px.
pub const BAR_NUMBER_SIZE: f32 = 12.0;
/// A bar number needs this much room to its right, so numbers never touch.
pub const BAR_NUMBER_MIN_SPACING_PX: f32 = 48.0;

// Cue flags: the guide is the theme's `cueMarkerWidth` at `cueGuideAlpha` of the deck colour.
pub const CUE_FLAG_WIDTH: f32 = 18.0;
pub const CUE_FLAG_HEIGHT: f32 = 16.0;
pub const CUE_NUMBER_SIZE: f32 = 12.0;
/// The main cue point (not one of the numbered hot cues).
pub const MAIN_CUE_COLOUR: Colour = Colour::from_argb(0xFFFF_9C78);
/// Hot cue 1..=8 colours; further cues repeat the cycle. Each keeps its hue family and is lightened
/// just enough that it is at least 60 degrees of hue from, or 0.25 relative luminance away from,
/// each of the three band colours (red, green, blue; the plan's disjointness rule, asserted in the
/// tests). The main cue follows the same rule. Hot cue 1 pink, 2 yellow, 3 green, 4 cyan, 5 violet,
/// 6 magenta-pink, 7 lime, 8 sky blue.
pub const CUE_PALETTE: [Colour; 8] = [
    Colour::from_argb(0xFFFF_98AA),
    Colour::from_argb(0xFFFF_D23F),
    Colour::from_argb(0xFF4D_D964),
    Colour::from_argb(0xFF22_D3EE),
    Colour::from_argb(0xFF99_8BFF),
    Colour::from_argb(0xFFFF_91D4),
    Colour::from_argb(0xFFB6_E83A),
    Colour::from_argb(0xFF56_9AFF),
];
// Loop region: the deck colour at the theme's `loopActiveAlpha` (idle: `loopIdleAlpha`), with
// 1 px edges (comp 4a).
pub const LOOP_EDGE_WIDTH: f32 = 1.0;
pub const LOOP_EDGE_ALPHA: f32 = 1.0;
/// A loop that is set but not engaged has dimmer edges.
pub const LOOP_IDLE_EDGE_ALPHA: f32 = 0.45;
/// Coincident cue flags stack at this offset (the flag height).
pub const CUE_STACK_STEP: f32 = 16.0;
/// Flags closer than this share a row only if they are further apart.
pub const CUE_STACK_RANGE: f32 = 18.0;
/// Rows of stacked flags: a 32 px strip (the smallest) fits two.
pub const CUE_ROWS: u8 = 2;

// Overview.
/// Height of an overview bar tick, in px.
pub const OVERVIEW_TICK_HEIGHT: f32 = 4.0;
pub const OVERVIEW_TICK_ALPHA: f32 = 0.35;
/// Bar ticks on the overview are thinned until they are at least this far apart.
pub const OVERVIEW_TICK_MIN_SPACING_PX: f32 = 8.0;
/// Side of the square flag on top of an overview cue guide, in px.
pub const OVERVIEW_CUE_FLAG: f32 = 4.0;
/// The Low-Band overview covers the played part with the ground colour at this alpha (comp 4a).
pub const OVERVIEW_PLAYED_SHADE_ALPHA: f32 = 0.6;

/// Colour of hot cue `index` (0-based): the pads' colour. The waveform draws cues in the deck
/// colour ([`StripInk::deck`]).
pub fn cue_colour(index: usize) -> Colour {
    CUE_PALETTE[index % CUE_PALETTE.len()]
}

/// Every colour and strength one deck's strips are drawn with, resolved from the theme. Cheap and
/// allocation-free, so it is built each frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripInk {
    pub style: WaveStyle,
    /// The strip ground (opaque).
    pub well: Colour,
    /// Grid lines and the Low-Band high layer: the text colour (white in the earlier styles).
    pub ink: Colour,
    /// Bar numbers (text, 4.5:1 on the well).
    pub number: Colour,
    /// The deck colour, at least [`LINE_CONTRAST`] on the well: low band, cue flags, loop.
    pub deck: Colour,
    /// Text on a deck-colour flag.
    pub on_deck: Colour,
    /// Signal red, at least [`LINE_CONTRAST`] on the well.
    pub playhead: Colour,
    pub playhead_width: f32,
    /// The halo either side of the playhead (earlier styles only).
    pub halo: Option<Colour>,
    /// Covers the played part of the overview (Low-Band); the earlier styles dim it instead.
    pub played_shade: Option<Colour>,
    pub cue_width: f32,
    pub cue_guide_alpha: f32,
    pub bar_alpha: f32,
    pub beat_alpha: f32,
    pub loop_alpha: f32,
    pub loop_idle_alpha: f32,
}

impl StripInk {
    /// The ink of deck `deck`'s strips in `style` under theme `t`.
    pub fn new(t: &ThemeTokens, style: WaveStyle, deck: usize) -> Self {
        let well = strip_well(t, style);
        let low_band = style == WaveStyle::LowBand;
        let deck_colour = ensure_contrast(t.accent_for(deck as i32), well, LINE_CONTRAST);
        Self {
            style,
            well,
            ink: if low_band {
                t.text_primary
            } else {
                GRID_COLOUR
            },
            number: if low_band {
                t.text_secondary
            } else {
                GRID_COLOUR.with_alpha(LEGACY_NUMBER_ALPHA)
            },
            deck: deck_colour,
            on_deck: clearer_on(deck_colour, t.on_accent, well),
            playhead: ensure_contrast(t.playhead, well, LINE_CONTRAST),
            playhead_width: t.playhead_width,
            halo: (!low_band).then_some(well),
            played_shade: low_band.then_some(t.ground),
            cue_width: t.cue_marker_width,
            cue_guide_alpha: t.cue_guide_alpha,
            bar_alpha: t.grid_bar_alpha,
            beat_alpha: t.grid_beat_alpha,
            loop_alpha: t.loop_active_alpha,
            loop_idle_alpha: t.loop_idle_alpha,
        }
    }

    /// The waveform palette for this ink.
    pub fn palette(&self) -> Palette {
        match self.style {
            WaveStyle::LowBand => Palette::low_band(self.well, self.deck, self.ink),
            style => Palette::new(style, self.well),
        }
    }

    /// A bar or beat line.
    pub fn grid(&self, bar: bool) -> Colour {
        let alpha = if bar { self.bar_alpha } else { self.beat_alpha };
        self.ink.with_alpha(alpha)
    }

    /// The cue guide line.
    pub fn cue_guide(&self) -> Colour {
        self.deck.with_alpha(self.cue_guide_alpha)
    }

    /// The loop fill: engaged, or set but idle.
    pub fn loop_fill(&self, looping: bool) -> Colour {
        let alpha = if looping {
            self.loop_alpha
        } else {
            self.loop_idle_alpha
        };
        self.deck.with_alpha(alpha)
    }

    /// The loop edges and its caption.
    pub fn loop_edge(&self, looping: bool) -> Colour {
        let alpha = if looping {
            LOOP_EDGE_ALPHA
        } else {
            LOOP_IDLE_EDGE_ALPHA
        };
        self.deck.with_alpha(alpha)
    }
}

/// Whichever of `a` and `b` contrasts more with `fill`.
fn clearer_on(fill: Colour, a: Colour, b: Colour) -> Colour {
    if contrast_ratio(a, fill) >= contrast_ratio(b, fill) {
        a
    } else {
        b
    }
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
    /// Vertical slot of the tab (0 at the top, [`CUE_STACK_STEP`] px per row): flags within
    /// [`CUE_STACK_RANGE`] px of an earlier one sit one row lower.
    pub row: u8,
}

/// Flags for the cues whose line or tab touches a strip `width` px wide. Unset cues are `None`.
pub fn cue_flags(xs: &[Option<f64>], width: f64) -> Vec<CueFlag> {
    let mut flags: Vec<CueFlag> = Vec::new();
    for (index, x) in xs.iter().enumerate() {
        let Some(x) = *x else { continue };
        let on_screen = x + f64::from(CUE_FLAG_WIDTH) >= 0.0 && x <= width;
        if !on_screen {
            continue;
        }
        // The first row no earlier flag within range occupies; the last row is shared when full.
        let row = (0..CUE_ROWS)
            .find(|row| {
                !flags.iter().any(|f| {
                    f.row == *row && (f64::from(f.x) - x).abs() < f64::from(CUE_STACK_RANGE)
                })
            })
            .unwrap_or(CUE_ROWS - 1);
        flags.push(CueFlag {
            index,
            x: x as f32,
            colour: cue_colour(index),
            label: (index + 1).to_string(),
            row,
        });
    }
    flags
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
    /// The loop's caption in the strip, its length in the label (comp 4a: "Loop · 4 bars"; the
    /// app counts loops in beats, as the loop buttons do). Empty without a length.
    pub fn caption(&self) -> String {
        match self.label.as_str() {
            "" => String::new(),
            "1" => "Loop · 1 beat".to_owned(),
            short if short.starts_with("1/") => format!("Loop · {short} beat"),
            label => format!("Loop · {label} beats"),
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

/// The playhead's x range, as (left, width): `width` px centred on `x`, snapped to whole pixels so
/// the line does not shimmer.
pub fn playhead_rect(x: f64, width: f32) -> (f32, f32) {
    ((x.round() as f32) - (width / 2.0).round(), width)
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
        assert_eq!(MAIN_CUE_COLOUR.argb(), 0xFFFF_9C78);
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
    fn bar_numbers_keep_their_room_at_every_zoom_and_width() {
        for width in [1280.0f32, 1920.0] {
            for seconds in crate::waveform::ZOOM_SECONDS {
                let bar_px = width * (60.0 / 124.0 * 4.0) / seconds as f32;
                let step = bar_label_step(bar_px);
                assert!(
                    step as f32 * bar_px >= BAR_NUMBER_MIN_SPACING_PX
                        || bar_px >= BAR_NUMBER_MIN_SPACING_PX,
                    "{width} px over {seconds} s: step {step}"
                );
            }
        }
        assert_eq!(bar_number(0.0, 4), Some(1), "numbers start at 1");
    }

    #[test]
    fn coincident_cues_stack_in_rows_inside_the_strip() {
        let flags = cue_flags(&[Some(100.0), Some(110.0), Some(300.0)], 800.0);
        assert_eq!(flags[0].row, 0);
        assert_eq!(flags[1].row, 1, "within 18 px of cue 1");
        assert_eq!(flags[2].row, 0);
        for f in &flags {
            // A 32 px strip is the smallest the zoomed waveform shows.
            assert!(f32::from(f.row) * CUE_STACK_STEP + CUE_FLAG_HEIGHT <= 32.0);
        }
    }

    #[test]
    fn cue_colours_keep_clear_of_the_band_hues() {
        // Plan: a cue colour is at least 60 degrees of hue from the three band hues, or differs
        // in luminance by 0.25. Reported by name so a failure says which cue.
        let bands = [
            crate::waveform::DARK_LOW,
            crate::waveform::DARK_MID,
            crate::waveform::DARK_HIGH,
        ];
        let mut close = Vec::new();
        for (i, cue) in CUE_PALETTE.iter().chain([&MAIN_CUE_COLOUR]).enumerate() {
            for band in bands {
                let hue_gap = {
                    let d = (hue_of(*cue) - hue_of(band)).rem_euclid(360.0);
                    d.min(360.0 - d)
                };
                let lum_gap = (crate::style::relative_luminance(*cue)
                    - crate::style::relative_luminance(band))
                .abs();
                if hue_gap < 60.0 && lum_gap < 0.25 {
                    close.push((i, band));
                }
            }
        }
        eprintln!("cue colours too close to a band hue: {close:?}");
        assert!(close.is_empty(), "{close:?}");
    }

    fn hue_of(c: Colour) -> f32 {
        let a = c.argb();
        let [r, g, b] = [16, 8, 0].map(|s| ((a >> s) & 0xFF) as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        if d <= 0.0 {
            return 0.0;
        }
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        h * 60.0
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
    fn an_idle_loop_is_dimmer_than_an_engaged_one_and_both_take_the_deck_colour() {
        let t = ThemeTokens::builtin_dark();
        let ink = StripInk::new(&t, WaveStyle::LowBand, 1);
        // Comp 4a: the deck colour at 14% with 1 px edges.
        assert!((ink.loop_fill(true).alpha() - 0.14).abs() < 0.01);
        assert!(ink.loop_fill(false).alpha() < ink.loop_fill(true).alpha());
        assert!(ink.loop_edge(false).alpha() < ink.loop_edge(true).alpha());
        assert_eq!(ink.loop_fill(true).with_alpha(1.0), t.accent_for(1));
        assert_eq!(LOOP_EDGE_WIDTH, 1.0);
    }

    #[test]
    fn the_loop_caption_names_its_length() {
        let region = |label: &str| LoopRegion {
            x0: 0.0,
            x1: 10.0,
            looping: true,
            label: label.into(),
        };
        assert_eq!(region("4").caption(), "Loop · 4 beats");
        assert_eq!(region("1").caption(), "Loop · 1 beat");
        assert_eq!(region("1/2").caption(), "Loop · 1/2 beat");
        assert_eq!(region("").caption(), "");
    }

    #[test]
    fn strip_ink_follows_the_comp_on_every_built_in_theme() {
        for (name, json) in crate::theme::BUILTIN_THEMES {
            let t = crate::theme::parse_theme(json).expect("parses").tokens;
            let ink = StripInk::new(&t, WaveStyle::LowBand, 0);
            assert_eq!(ink.well, crate::style::over(t.well, t.ground), "{name}");
            assert_eq!(ink.ink, t.text_primary, "{name}: grid in the text colour");
            assert_eq!(ink.playhead, t.playhead, "{name}: signal red as is");
            assert_eq!(ink.deck, t.accent_for(0), "{name}: deck colour as is");
            assert_eq!(ink.halo, None, "{name}: no halo in Low-Band");
            assert_eq!((ink.bar_alpha, ink.beat_alpha), (0.14, 0.05), "{name}");
            assert_eq!((ink.cue_width, ink.cue_guide_alpha), (1.0, 0.45), "{name}");
            assert_eq!(ink.playhead_width, 2.0, "{name}");
            assert!(
                contrast_ratio(ink.number, ink.well) >= 4.5,
                "{name}: bar numbers"
            );
            assert!(
                contrast_ratio(ink.on_deck, ink.deck) >= 4.5,
                "{name}: cue numbers"
            );
            for style in WaveStyle::ALL {
                let ink = StripInk::new(&t, style, 1);
                for (what, c) in [("deck", ink.deck), ("playhead", ink.playhead)] {
                    let r = contrast_ratio(c, ink.well);
                    assert!(r >= LINE_CONTRAST, "{name} {style:?} {what}: {r:.2}");
                }
            }
        }
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
    fn grid_lines_are_the_text_colour_at_14_and_5_percent() {
        let t = ThemeTokens::builtin_dark();
        let ink = StripInk::new(&t, WaveStyle::LowBand, 0);
        assert!((ink.grid(false).alpha() - 0.05).abs() < 0.01);
        assert!((ink.grid(true).alpha() - 0.14).abs() < 0.01);
        assert_eq!(ink.grid(true).with_alpha(1.0), t.text_primary);
        assert_eq!((BEAT_LINE_WIDTH, BAR_LINE_WIDTH), (1.0, 1.0));
        let legacy = StripInk::new(&t, WaveStyle::Colour, 0);
        assert_eq!(legacy.grid(true).with_alpha(1.0), GRID_COLOUR);
    }

    #[test]
    fn playhead_is_two_signal_red_pixels() {
        let t = ThemeTokens::builtin_dark();
        let ink = StripInk::new(&t, WaveStyle::LowBand, 0);
        assert_eq!(ink.playhead, t.playhead);
        assert_eq!(ink.playhead.alpha(), 1.0);
        let (left, width) = playhead_rect(200.0, ink.playhead_width);
        assert_eq!((left, width), (199.0, 2.0), "bar spans 199..201");
        assert_eq!(
            playhead_rect(200.4, 2.0),
            playhead_rect(200.0, 2.0),
            "snaps to pixels"
        );
        let legacy = StripInk::new(&t, WaveStyle::Colour, 0);
        assert_eq!(legacy.halo, Some(legacy.well), "a halo over the red band");
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
