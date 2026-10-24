//! One mixer channel, top to bottom: gain, a three-band EQ whose labels are the kill switches,
//! filter, PFL and the FX shortcut, the channel fader beside its meter, and the crossfader
//! assignment as an A / THRU / B switch. Routes `mixer.ch<N>.<id>`. State, routing and layout.

use super::px;
use crate::binding::{assign_from_index, assign_index, LevelSource, MixerBinding};
use crate::control::{Knob, Ranged, Readout};
use crate::geometry::Rect;
use crate::meter::{column_width, MeterParams, StereoMeter};
use crate::theme::ThemeTokens;
use om_core::param_id::{ChannelParam, EqBand};
use om_dsp::CrossfaderAssign;

/// EQ bands in engine order: low, mid, high.
pub const BANDS: usize = 3;
pub const EQ_BANDS: [EqBand; BANDS] = [EqBand::Low, EqBand::Mid, EqBand::High];
/// Band names as printed on the strip, in engine order. The label is also the kill switch.
pub const EQ_LABELS: [&str; BANDS] = ["LOW", "MID", "HI"];
/// Knob labels: printed once, 2 to 6 letters.
pub const TRIM_LABEL: &str = "GAIN";
pub const FILTER_LABEL: &str = "FILTER";
/// Crossfader assignment switch, indexed by [`CrossfaderAssign`] index: Thru, Left, Right. On
/// screen it reads A | THRU | B, the sides the crossfader's ends name.
pub const ASSIGN_LABELS: [&str; 3] = ["THRU", "A", "B"];
/// Kill drops a band to this floor; a band at or below it reads as killed.
pub const KILL_TOLERANCE_DB: f64 = 0.01;

/// Gap between the strip's rows, in px.
pub const STRIP_GAP: i32 = 6;
/// Height of the gain and filter knobs (a full-width bar), in px.
pub const WIDE_KNOB_HEIGHT: i32 = 40;
/// Height of a band's label (its kill switch) and of its knob, in px.
pub const BAND_LABEL_HEIGHT: i32 = 28;
pub const BAND_KNOB_HEIGHT: i32 = 56;
/// Height of the PFL and FX row (primary controls), in px.
pub const PRIMARY_HEIGHT: i32 = 44;
/// Height of the crossfader assignment switch, in px.
pub const SWITCH_HEIGHT: i32 = 32;
/// Height of the title row (the deck letter), in px.
pub const TITLE_HEIGHT: i32 = 20;
/// The fader is never shorter than this, in px; below it the knob rows give way.
pub const MIN_FADER_HEIGHT: i32 = 80;

/// The letter a channel carries: its deck, `A`, `B`, `C`...
pub fn channel_letter(channel: usize) -> char {
    char::from(b'A' + (channel % 26) as u8)
}

/// Rectangles of one channel strip, top to bottom: title, gain, the three bands (label above knob),
/// filter, PFL and FX, fader beside the meter, crossfader assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelLayout {
    pub title: Rect,
    pub trim: Rect,
    pub filter: Rect,
    /// Indexed by band (engine order), though displayed Hi | Mid | Lo.
    pub eq: [Rect; BANDS],
    /// Each band's label; pressing it kills the band.
    pub kill: [Rect; BANDS],
    pub pfl: Rect,
    pub fx: Rect,
    /// Indexed by [`CrossfaderAssign`] index: Thru, Left, Right.
    pub assign: [Rect; 3],
    pub fader: Rect,
    /// The meter column (dB labels, when there is room, and two bars).
    pub meter: Rect,
    /// Whether the column has room for the dB labels.
    pub meter_labels: bool,
}

pub struct ChannelStrip {
    binding: MixerBinding,
    channel: usize,
    pub trim: Knob,
    pub filter: Knob,
    pub eq: [Knob; BANDS],
    pub fader: Ranged,
    pub pfl: bool,
    /// Crossfader assignment index (see [`assign_index`]).
    pub assign: usize,
    pub meter: StereoMeter,
    /// EQ value to return to when a kill is released.
    restore_db: [f64; BANDS],
}

impl ChannelStrip {
    /// Builds the controls and reads the engine once. Nothing is sent.
    pub fn new(binding: MixerBinding, channel: usize, meter: MeterParams) -> Self {
        let min_db = f64::from(om_dsp::ThreeBandEq::MIN_DB);
        let max_db = f64::from(om_dsp::ThreeBandEq::MAX_DB);
        let mut strip = Self {
            binding,
            channel,
            trim: Knob::new(TRIM_LABEL, Readout::Gain, 0.0, 4.0, 1.0).with_detent(),
            filter: Knob::new(FILTER_LABEL, Readout::Filter, -1.0, 1.0, 0.0).with_detent(),
            eq: EQ_LABELS.map(|label| {
                Knob::new(label, Readout::Decibels, min_db, max_db, 0.0).with_detent()
            }),
            fader: Ranged::new(0.0, 1.0, 0.001, 1.0),
            pfl: false,
            assign: 0,
            meter: StereoMeter::new(meter),
            restore_db: [0.0; BANDS],
        };
        strip.pull(false);
        strip
    }

    pub fn channel(&self) -> usize {
        self.channel
    }

    /// Reads the engine into every control. The fader is left alone while the user drags it.
    pub fn pull(&mut self, dragging_fader: bool) {
        let m = self.binding.mixer();
        let ch = self.channel;
        self.trim.set_quiet(f64::from(m.trim(ch)));
        self.filter.set_quiet(f64::from(m.filter(ch)));
        for (band, knob) in self.eq.iter_mut().enumerate() {
            knob.set_quiet(f64::from(m.eq(ch, band)));
        }
        if !dragging_fader {
            self.fader.set(f64::from(m.fader(ch)));
        }
        self.pfl = m.pfl(ch);
        self.assign = assign_index(m.crossfader_assign(ch));
    }

    /// One UI frame: pulls the engine state and advances the meter by `dt` seconds.
    pub fn refresh(&mut self, dt: f32, levels: &dyn LevelSource, dragging_fader: bool) {
        self.pull(dragging_fader);
        let (left, right) = levels.channel_peaks(self.channel);
        self.meter.update(left, right, dt);
    }

    fn send(&self, p: ChannelParam, value: f64) {
        self.binding.send_channel(self.channel, p, value);
    }

    pub fn trim_changed(&mut self, value: f64) -> f64 {
        let v = self.trim.range.set(value);
        self.send(ChannelParam::Trim, v);
        v
    }

    pub fn filter_changed(&mut self, value: f64) -> f64 {
        let v = self.filter.range.set(value);
        self.send(ChannelParam::Filter, v);
        v
    }

    pub fn eq_changed(&mut self, band: usize, value: f64) -> f64 {
        let v = self.eq[band].range.set(value);
        self.send(ChannelParam::Eq(EQ_BANDS[band]), v);
        v
    }

    /// The fader moved: stores the snapped value and sends it.
    pub fn fader_changed(&mut self, value: f64) -> f64 {
        let v = self.fader.set(value);
        self.send(ChannelParam::Fader, v);
        v
    }

    /// Whether a band is killed (at the engine floor).
    pub fn is_killed(&self, band: usize) -> bool {
        f64::from(self.binding.mixer().eq(self.channel, band))
            <= f64::from(om_dsp::ThreeBandEq::MIN_DB) + KILL_TOLERANCE_DB
    }

    /// Kill drops the band to the floor and remembers where it was; releasing goes back there.
    pub fn kill_pressed(&mut self, band: usize) {
        if self.is_killed(band) {
            self.send(ChannelParam::Eq(EQ_BANDS[band]), self.restore_db[band]);
        } else {
            self.restore_db[band] = f64::from(self.binding.mixer().eq(self.channel, band));
            self.send(
                ChannelParam::Eq(EQ_BANDS[band]),
                f64::from(om_dsp::ThreeBandEq::MIN_DB),
            );
        }
        self.pull(false);
    }

    /// PFL toggles the channel into the headphone cue.
    pub fn pfl_pressed(&mut self) -> bool {
        let on = !self.binding.mixer().pfl(self.channel);
        self.send(ChannelParam::Pfl, if on { 1.0 } else { 0.0 });
        self.pull(false);
        on
    }

    /// Crossfader assignment buttons (index as [`assign_index`]). Out-of-range indices do nothing.
    pub fn assign_pressed(&mut self, index: usize) -> bool {
        let Some(assign) = assign_from_index(index) else {
            return false;
        };
        self.send(ChannelParam::Xfader, assign_index(assign) as f64);
        self.pull(false);
        true
    }

    /// The FX button: the caller shows this channel's rack.
    pub fn fx_pressed(&self) -> usize {
        self.channel
    }

    /// The crossfader assignment the engine reports, for display.
    pub fn assignment(&self) -> CrossfaderAssign {
        self.binding.mixer().crossfader_assign(self.channel)
    }

    /// Rectangles of the strip for a `bounds` card.
    pub fn layout(bounds: Rect, theme: &ThemeTokens) -> ChannelLayout {
        let mut area = bounds.reduced(px(theme.mixer_pad_x), px(theme.mixer_pad_y));
        let m = StripMetrics::for_height(area.h);

        let title = area.remove_top(TITLE_HEIGHT);
        area.remove_top(STRIP_GAP);

        // Gain, full width.
        let trim = area.remove_top(m.wide_knob);
        area.remove_top(STRIP_GAP);

        // Bands: label (the kill switch) over knob, displayed Hi | Mid | Lo as on a hardware mixer.
        let mut labels = area.remove_top(m.band_label);
        let mut knobs = area.remove_top(m.band_knob);
        area.remove_top(STRIP_GAP);
        let cell = knobs.w / BANDS as i32;
        let mut eq = [Rect::default(); BANDS];
        let mut kill = [Rect::default(); BANDS];
        for i in 0..BANDS {
            let band = BANDS - 1 - i;
            kill[band] = labels.remove_left(cell).reduced(1, 0);
            eq[band] = knobs.remove_left(cell).reduced(1, 0);
        }

        // Filter, full width.
        let filter = area.remove_top(m.wide_knob);
        area.remove_top(STRIP_GAP);

        // PFL and the FX shortcut: the primary controls.
        let mut row = area.remove_top(m.primary);
        area.remove_top(STRIP_GAP);
        let pfl = row.remove_left(row.w * 3 / 5).reduced(1, 0);
        let fx = row.reduced(1, 0);

        // The crossfader assignment sits at the foot; the fader and meter take what is left.
        let mut foot = area.remove_bottom(SWITCH_HEIGHT);
        area.remove_bottom(STRIP_GAP);
        let third = foot.w / 3;
        let mut assign = [Rect::default(); 3];
        assign[1] = foot.remove_left(third);
        assign[0] = foot.remove_left(third);
        assign[2] = foot;

        let fader_w = px(theme.mixer_fader_width);
        let with_labels =
            area.w >= fader_w + STRIP_GAP + column_width(theme.mixer_meter_width, true);
        let meter_w = column_width(theme.mixer_meter_width, with_labels);
        let mut group =
            area.with_size_keeping_centre(area.w.min(fader_w + STRIP_GAP + meter_w), area.h);
        let meter = group.remove_right(meter_w);
        group.remove_right(STRIP_GAP);

        ChannelLayout {
            title,
            trim,
            filter,
            eq,
            kill,
            pfl,
            fx,
            assign,
            fader: group,
            meter,
            meter_labels: with_labels,
        }
    }
}

/// Heights of the strip's rows, chosen from the height the strip has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripMetrics {
    pub wide_knob: i32,
    pub band_label: i32,
    pub band_knob: i32,
    pub primary: i32,
}

impl StripMetrics {
    /// The full-size rows.
    pub const FULL: Self = Self {
        wide_knob: WIDE_KNOB_HEIGHT,
        band_label: BAND_LABEL_HEIGHT,
        band_knob: BAND_KNOB_HEIGHT,
        primary: PRIMARY_HEIGHT,
    };
    /// The rows on a short window: every control stays at least 32 px (the band label 28, being
    /// text over the knob it belongs to) rather than shrinking further.
    pub const COMPACT: Self = Self {
        wide_knob: 32,
        band_label: 28,
        band_knob: 40,
        primary: 36,
    };

    /// Height of everything but the fader, with the gaps between rows.
    pub fn fixed_height(&self) -> i32 {
        // Rows: title, gain, band label + knob (one row group), filter, PFL, fader, assign.
        TITLE_HEIGHT
            + self.wide_knob
            + self.band_label
            + self.band_knob
            + self.wide_knob
            + self.primary
            + SWITCH_HEIGHT
            + STRIP_GAP * 6
    }

    /// The full rows when they leave the fader [`MIN_FADER_HEIGHT`], else the compact ones.
    pub fn for_height(inner_height: i32) -> Self {
        if inner_height - Self::FULL.fixed_height() >= MIN_FADER_HEIGHT {
            Self::FULL
        } else {
            Self::COMPACT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_short_and_read_a_thru_b() {
        assert_eq!(EQ_LABELS, ["LOW", "MID", "HI"]);
        assert_eq!(ASSIGN_LABELS, ["THRU", "A", "B"]);
        for label in EQ_LABELS
            .iter()
            .chain([&TRIM_LABEL, &FILTER_LABEL])
            .chain(ASSIGN_LABELS.iter())
        {
            assert!(label.len() <= 6 && label.chars().all(|c| c.is_ascii_uppercase()));
        }
        assert_eq!((channel_letter(0), channel_letter(1)), ('A', 'B'));
    }

    #[test]
    fn tall_strips_get_full_rows_and_short_ones_compact_rows() {
        let tall = StripMetrics::for_height(StripMetrics::FULL.fixed_height() + 100);
        assert_eq!(tall, StripMetrics::FULL);
        let short = StripMetrics::for_height(StripMetrics::FULL.fixed_height() + 20);
        assert_eq!(short, StripMetrics::COMPACT);
        assert!(StripMetrics::COMPACT.fixed_height() < StripMetrics::FULL.fixed_height());
    }

    #[test]
    fn the_layout_keeps_every_control_at_least_32px_and_the_fader_travelling() {
        let theme = ThemeTokens::builtin_dark();
        for (w, h) in [(145, 440), (145, 380), (200, 600), (130, 400)] {
            let l = ChannelStrip::layout(Rect::sized(w, h), &theme);
            for (name, r) in [("trim", l.trim), ("filter", l.filter), ("pfl", l.pfl)] {
                assert!(r.h >= 32, "{w}x{h}: {name} is {} px", r.h);
            }
            for r in l.eq {
                assert!(r.h >= 36, "{w}x{h}: a band knob is {} px", r.h);
            }
            assert!(l.fader.h >= 60, "{w}x{h}: fader {}", l.fader.h);
            assert!(l.fader.w + l.meter.w <= w);
        }
    }

    #[test]
    fn a_wide_strip_prints_the_meter_scale_and_a_narrow_one_drops_it() {
        let theme = ThemeTokens::builtin_dark();
        assert!(ChannelStrip::layout(Rect::sized(160, 440), &theme).meter_labels);
        assert!(!ChannelStrip::layout(Rect::sized(100, 440), &theme).meter_labels);
    }

    #[test]
    fn the_assignment_switch_reads_a_thru_b_left_to_right() {
        let theme = ThemeTokens::builtin_dark();
        let l = ChannelStrip::layout(Rect::sized(145, 440), &theme);
        // Indexed Thru, Left, Right; on screen A (left), THRU, B (right).
        assert!(l.assign[1].x < l.assign[0].x && l.assign[0].x < l.assign[2].x);
    }
}
