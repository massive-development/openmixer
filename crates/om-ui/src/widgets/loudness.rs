//! Loudness meter model: an EBU R128 reading as bar fills, readout strings, the true-peak clip
//! flag and the dropped-frames warning, plus the panel's layout. No drawing, no engine handle: the
//! window polls `LoudnessControl::reading()` on its tick and passes the value to
//! [`LoudnessPanel::set_reading`].

use super::CAPTION_HEIGHT;
use crate::geometry::Rect;
use om_engine::LoudnessReading;

/// Shown for a value that has not been measured (`-inf` or not finite).
pub const NO_VALUE: &str = "\u{2014}";
/// Bottom of the momentary and short-term bars, in LUFS.
pub const BAR_FLOOR_LUFS: f32 = -36.0;
/// Top of the bars, in LUFS.
pub const BAR_CEILING_LUFS: f32 = 0.0;
/// A true peak above this lights the clip lamp (the EBU R128 permitted maximum level, -1 dBTP).
pub const TRUE_PEAK_LIMIT_DBTP: f32 = -1.0;
/// Height of a row of text or a bar, in px.
pub const ROW_HEIGHT: i32 = 16;
/// Gap between rows, in px.
pub const ROW_GAP: i32 = 2;
/// Width of the bar rows' label and of their value, in px.
const LABEL_WIDTH: i32 = 14;
const VALUE_WIDTH: i32 = 38;
/// Width of the Reset button, in px.
const RESET_WIDTH: i32 = 44;
/// Side of the clip lamp, in px.
const LAMP_SIZE: i32 = 8;
/// Width of the panel's toggle button, in px.
const TOGGLE_BUTTON_WIDTH: i32 = 48;
/// Height of the panel's toggle button, in px.
pub const TOGGLE_HEIGHT: i32 = 24;

/// A level in LUFS or dBTP as text with one decimal, or an em dash when nothing was measured.
pub fn level_text(value: f32) -> String {
    if value.is_finite() {
        format!("{value:.1}")
    } else {
        NO_VALUE.to_owned()
    }
}

/// Where `lufs` falls on the bars, 0 to 1; a value that is not finite is empty.
pub fn bar_fill(lufs: f32) -> f32 {
    if !lufs.is_finite() {
        return 0.0;
    }
    ((lufs - BAR_FLOOR_LUFS) / (BAR_CEILING_LUFS - BAR_FLOOR_LUFS)).clamp(0.0, 1.0)
}

/// Everything the panel draws, derived from one [`LoudnessReading`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessView {
    pub momentary_fill: f32,
    pub short_term_fill: f32,
    pub momentary_text: String,
    pub short_term_text: String,
    pub integrated_text: String,
    /// Em dash until the gate has passed a block, as the range means nothing before.
    pub range_text: String,
    /// The higher channel's true peak.
    pub true_peak_text: String,
    pub clipping: bool,
    /// Set when the meter lost frames since the last reset.
    pub warning: Option<String>,
}

impl LoudnessView {
    pub fn from_reading(r: &LoudnessReading) -> Self {
        let peak = r.true_peak_max_dbtp();
        let range_text = if r.integrated_lufs.is_finite() {
            level_text(r.loudness_range_lu)
        } else {
            NO_VALUE.to_owned()
        };
        Self {
            momentary_fill: bar_fill(r.momentary_lufs),
            short_term_fill: bar_fill(r.short_term_lufs),
            momentary_text: level_text(r.momentary_lufs),
            short_term_text: level_text(r.short_term_lufs),
            integrated_text: level_text(r.integrated_lufs),
            range_text,
            true_peak_text: level_text(peak),
            clipping: peak.is_finite() && peak > TRUE_PEAK_LIMIT_DBTP,
            warning: (r.dropped_frames > 0).then(|| dropped_text(r.dropped_frames)),
        }
    }
}

fn dropped_text(frames: u64) -> String {
    format!("{frames} frames dropped")
}

/// A bar row: its label, the bar and the value printed after it.
#[derive(Debug, Clone, PartialEq)]
pub struct BarRow {
    pub label: Rect,
    pub bar: Rect,
    pub value: Rect,
}

/// Rectangles of the open panel, top to bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessLayout {
    pub title: Rect,
    pub reset: Rect,
    pub momentary: BarRow,
    pub short_term: BarRow,
    pub integrated: Rect,
    pub range: Rect,
    pub true_peak: Rect,
    pub clip_lamp: Rect,
    pub warning: Rect,
}

/// Height the open panel needs, in px.
pub const PANEL_HEIGHT: i32 = ROW_HEIGHT * 5 + CAPTION_HEIGHT + 6 * ROW_GAP;

/// The loudness panel: the latest reading as a view, and whether the panel is open.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessPanel {
    view: LoudnessView,
    open: bool,
}

impl Default for LoudnessPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl LoudnessPanel {
    pub fn new() -> Self {
        Self {
            view: LoudnessView::from_reading(&LoudnessReading::SILENT),
            open: false,
        }
    }

    pub fn set_reading(&mut self, reading: &LoudnessReading) {
        self.view = LoudnessView::from_reading(reading);
    }

    pub fn view(&self) -> &LoudnessView {
        &self.view
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// The line beside the toggle button: the momentary reading, flagged when frames were dropped.
    pub fn summary_text(&self) -> String {
        let prefix = if self.view.warning.is_some() {
            "! "
        } else {
            ""
        };
        format!("{prefix}M {}", self.view.momentary_text)
    }

    /// The toggle strip: the button on the left, the summary text filling the rest.
    pub fn toggle_layout(bounds: Rect) -> (Rect, Rect) {
        let mut rest = bounds;
        let button = rest.remove_left(TOGGLE_BUTTON_WIDTH.min(rest.w));
        rest.remove_left(ROW_GAP * 2);
        (button, rest)
    }

    pub fn layout(bounds: Rect) -> LoudnessLayout {
        let mut area = bounds;
        let mut top = area.remove_top(CAPTION_HEIGHT.max(ROW_HEIGHT));
        let reset = top.remove_right(RESET_WIDTH.min(top.w));
        let mut rows = Rows::new(area);
        let momentary = rows.bar_row();
        let short_term = rows.bar_row();
        let mut stats = rows.next();
        let integrated = stats.remove_left(stats.w / 2);
        let mut peak = rows.next();
        let clip_lamp = peak.remove_right(LAMP_SIZE.min(peak.w));
        LoudnessLayout {
            title: top,
            reset,
            momentary,
            short_term,
            integrated,
            range: stats,
            true_peak: peak,
            clip_lamp: Rect::new(
                clip_lamp.x,
                clip_lamp.y + (ROW_HEIGHT - LAMP_SIZE) / 2,
                clip_lamp.w,
                LAMP_SIZE,
            ),
            warning: rows.next(),
        }
    }
}

/// Hands out `ROW_HEIGHT` rows separated by `ROW_GAP`.
struct Rows {
    area: Rect,
}

impl Rows {
    fn new(mut area: Rect) -> Self {
        area.remove_top(ROW_GAP);
        Self { area }
    }

    fn next(&mut self) -> Rect {
        let row = self.area.remove_top(ROW_HEIGHT);
        self.area.remove_top(ROW_GAP);
        row
    }

    fn bar_row(&mut self) -> BarRow {
        let mut row = self.next();
        let label = row.remove_left(LABEL_WIDTH.min(row.w));
        let value = row.remove_right(VALUE_WIDTH.min(row.w));
        BarRow {
            label,
            bar: row.reduced(2, 3),
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading() -> LoudnessReading {
        LoudnessReading {
            momentary_lufs: -14.04,
            short_term_lufs: -18.0,
            integrated_lufs: -16.25,
            loudness_range_lu: 5.5,
            true_peak_dbtp: [-3.0, -0.4],
            dropped_frames: 0,
            frames_measured: 480_000,
        }
    }

    #[test]
    fn nothing_measured_prints_an_em_dash_and_an_empty_bar() {
        let v = LoudnessView::from_reading(&LoudnessReading::SILENT);
        for text in [
            &v.momentary_text,
            &v.short_term_text,
            &v.integrated_text,
            &v.range_text,
            &v.true_peak_text,
        ] {
            assert_eq!(text, "\u{2014}");
        }
        assert_eq!((v.momentary_fill, v.short_term_fill), (0.0, 0.0));
        assert!(!v.clipping && v.warning.is_none());
    }

    #[test]
    fn values_print_with_one_decimal() {
        let v = LoudnessView::from_reading(&reading());
        assert_eq!(v.momentary_text, "-14.0");
        assert_eq!(v.short_term_text, "-18.0");
        assert_eq!(v.integrated_text, "-16.2");
        assert_eq!(v.range_text, "5.5");
        assert_eq!(v.true_peak_text, "-0.4", "the higher channel");
    }

    #[test]
    fn the_range_waits_for_the_gate() {
        let mut r = reading();
        r.integrated_lufs = f32::NEG_INFINITY;
        assert_eq!(LoudnessView::from_reading(&r).range_text, NO_VALUE);
    }

    #[test]
    fn nan_is_not_a_value() {
        assert_eq!(level_text(f32::NAN), NO_VALUE);
        assert_eq!(bar_fill(f32::NAN), 0.0);
    }

    #[test]
    fn bars_clamp_to_the_scale() {
        assert_eq!(bar_fill(BAR_FLOOR_LUFS), 0.0);
        assert_eq!(bar_fill(BAR_CEILING_LUFS), 1.0);
        assert_eq!(bar_fill(-80.0), 0.0);
        assert_eq!(bar_fill(6.0), 1.0);
        assert!((bar_fill(-18.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_clip_lamp_lights_above_the_limit_only() {
        let mut r = reading();
        r.true_peak_dbtp = [TRUE_PEAK_LIMIT_DBTP, f32::NEG_INFINITY];
        assert!(!LoudnessView::from_reading(&r).clipping, "at the limit");
        r.true_peak_dbtp = [f32::NEG_INFINITY, 0.3];
        let v = LoudnessView::from_reading(&r);
        assert!(v.clipping);
        assert_eq!(v.true_peak_text, "0.3");
    }

    #[test]
    fn dropped_frames_raise_a_warning() {
        let mut r = reading();
        r.dropped_frames = 480;
        let v = LoudnessView::from_reading(&r);
        assert_eq!(v.warning.as_deref(), Some("480 frames dropped"));
        let mut panel = LoudnessPanel::new();
        panel.set_reading(&r);
        assert_eq!(panel.summary_text(), "! M -14.0");
        r.dropped_frames = 0;
        panel.set_reading(&r);
        assert_eq!(panel.summary_text(), "M -14.0");
    }

    #[test]
    fn the_toggle_strip_holds_a_button_and_its_summary() {
        let (button, summary) = LoudnessPanel::toggle_layout(Rect::new(10, 20, 109, TOGGLE_HEIGHT));
        assert!(!button.intersects(&summary));
        assert!(summary.w >= 40 && button.h == TOGGLE_HEIGHT);
    }

    #[test]
    fn the_toggle_opens_and_closes_the_panel() {
        let mut panel = LoudnessPanel::default();
        assert!(!panel.is_open());
        panel.toggle();
        assert!(panel.is_open());
        panel.toggle();
        assert!(!panel.is_open());
    }

    #[test]
    fn the_layout_fits_its_height_without_overlap() {
        let bounds = Rect::sized(109, PANEL_HEIGHT);
        let l = LoudnessPanel::layout(bounds);
        let rects = [
            l.title,
            l.reset,
            l.momentary.label,
            l.momentary.bar,
            l.momentary.value,
            l.short_term.label,
            l.short_term.bar,
            l.short_term.value,
            l.integrated,
            l.range,
            l.true_peak,
            l.clip_lamp,
            l.warning,
        ];
        for (i, a) in rects.iter().enumerate() {
            assert!(!a.is_empty(), "rect {i} is empty");
            assert!(bounds.contains(a), "rect {i} {a:?} leaves the panel");
            for (j, b) in rects.iter().enumerate().skip(i + 1) {
                assert!(!a.intersects(b), "rects {i} and {j} overlap");
            }
        }
        assert!(l.momentary.bar.w >= 40, "bar {} px", l.momentary.bar.w);
    }

    #[test]
    fn the_mixer_module_leaves_room_for_the_toggle_and_the_open_panel() {
        use crate::theme::ThemeTokens;
        use crate::widgets::mixer::MixerPanel;
        let theme = ThemeTokens::builtin_dark();
        for (w, h) in [(253, 262), (386, 354)] {
            let l = MixerPanel::module_layout(Rect::sized(w, h), &theme);
            let card = Rect::sized(w, h);
            assert!(card.contains(&l.loudness_panel), "{w}x{h}");
            assert!(l.loudness.h >= 20, "{w}x{h}: toggle {} px", l.loudness.h);
            assert!(!l.loudness.intersects(&l.fx), "{w}x{h}");
            assert!(!l.loudness.intersects(&l.loudness_panel));
            assert!(l.loudness_panel.h >= PANEL_HEIGHT, "{w}x{h}");
            assert!(!l.loudness_panel.intersects(&l.master));
            assert!(!l.loudness_panel.intersects(&l.crossfader));
        }
    }
}
