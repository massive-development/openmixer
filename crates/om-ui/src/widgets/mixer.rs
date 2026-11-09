//! The mixer: one channel per deck, master, crossfader, headphone cue and the effects, in the mixer
//! column between the decks' controls (comp 4a/4b). Page state, refresh and the column layout. The
//! frame clock belongs to the gpui layer.

use super::channel::ChannelStrip;
use super::crossfader::CrossfaderSection;
use super::cue::CueSection;
use super::fx_panel::FxPanel;
use super::master::MasterSection;
use crate::binding::{LevelSource, MixerBinding};
use crate::geometry::Rect;
use crate::meter::MeterParams;
use crate::theme::DensityMetrics;

/// A stalled UI should not blank the meters in one step.
pub const MAX_FRAME_SECONDS: f32 = 0.25;

/// Which page the mixer slot shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Mixer,
    Fx,
}

pub struct MixerPanel {
    binding: MixerBinding,
    page: Page,
    pub strips: Vec<ChannelStrip>,
    pub master: MasterSection,
    pub crossfader: CrossfaderSection,
    pub cue: CueSection,
    pub fx: FxPanel,
}

impl MixerPanel {
    /// Builds every control for the engine's channels and shows the mixer page.
    pub fn new(binding: MixerBinding, meter: MeterParams) -> Self {
        let strips = (0..binding.num_channels())
            .map(|ch| ChannelStrip::new(binding.clone(), ch, meter))
            .collect();
        Self {
            page: Page::Mixer,
            strips,
            master: MasterSection::new(binding.clone(), meter),
            crossfader: CrossfaderSection::new(binding.clone()),
            cue: CueSection::new(binding.clone()),
            fx: FxPanel::new(binding.clone()),
            binding,
        }
    }

    pub fn page(&self) -> Page {
        self.page
    }

    /// Mixer strips and sections are shown on the mixer page only.
    pub fn mixer_visible(&self) -> bool {
        self.page == Page::Mixer
    }

    pub fn fx_visible(&self) -> bool {
        self.page == Page::Fx
    }

    pub fn set_page(&mut self, page: Page) {
        self.page = page;
        if page == Page::Fx {
            self.fx.refresh();
        }
    }

    /// A strip's FX button: show that channel's rack on the FX page.
    pub fn request_fx(&mut self, channel: usize) {
        self.fx.show_channel(channel);
        self.set_page(Page::Fx);
    }

    /// One frame: mirrors the engine and advances the meters by `dt` seconds (clamped to
    /// [`MAX_FRAME_SECONDS`]). The strips and the module stay on screen on the FX page, so they
    /// keep updating; the effects are refreshed as well while that page is open.
    pub fn refresh(&mut self, dt: f32, levels: &dyn LevelSource, dragging_fader: bool) {
        if self.page == Page::Fx {
            self.fx.refresh();
        }
        let dt = dt.clamp(0.0, MAX_FRAME_SECONDS);
        for strip in &mut self.strips {
            strip.refresh(dt, levels, dragging_fader);
        }
        self.master.refresh(dt, levels, dragging_fader);
        self.crossfader.pull(dragging_fader);
        self.cue.pull(false, false);
    }

    pub fn binding(&self) -> &MixerBinding {
        &self.binding
    }
}

/// A knob with its caption block beside it: the label over the value in mono, on the knob's outer
/// side (left of deck A's knobs, right of deck B's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KnobCell {
    pub knob: Rect,
    /// The label and the value; for an EQ band it is also the band's kill switch.
    pub text: Rect,
}

impl KnobCell {
    /// The cell's whole area: the knob and its text.
    pub fn bounds(&self) -> Rect {
        let x0 = self.knob.x.min(self.text.x);
        let x1 = self.knob.right().max(self.text.right());
        let y0 = self.knob.y.min(self.text.y);
        let y1 = self.knob.bottom().max(self.text.bottom());
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// Rows of a channel's knob column, top to bottom.
pub const KNOB_ROWS: usize = 5;
/// Which control each knob row holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobRow {
    Gain,
    /// An EQ band by index into [`super::channel::EQ_BANDS`] (0 low, 1 mid, 2 high).
    Band(usize),
    Filter,
}

/// Gain, high, mid, low, filter, as on a hardware channel.
pub const KNOB_ORDER: [KnobRow; KNOB_ROWS] = [
    KnobRow::Gain,
    KnobRow::Band(2),
    KnobRow::Band(1),
    KnobRow::Band(0),
    KnobRow::Filter,
];

/// One channel's part of the mixer column.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelColumn {
    /// In [`KNOB_ORDER`].
    pub knobs: [KnobCell; KNOB_ROWS],
    pub fader: Rect,
    pub meter: Rect,
    /// The headphone cue (PFL) button.
    pub cue: Rect,
    /// The crossfader assignment (Thru, A or B), one press to the next.
    pub assign: Rect,
}

/// Rectangles of the mixer column (comp 4a/4b mixer, plus the controls the comp leaves out): the
/// channels' knob columns at the outer edges with their faders and meters inside them, the master
/// fader and meter in the middle, then the crossfader between the A and B tags, a row with each
/// channel's cue and assignment around the curve switch, and a row with the headphone knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnLayout {
    /// Channel A (index 0) on the left, B mirrored on the right.
    pub channels: [ChannelColumn; 2],
    pub master_fader: Rect,
    pub master_meter: Rect,
    /// The A and B tags at the crossfader's ends.
    pub tags: [Rect; 2],
    pub crossfader: Rect,
    /// The crossfader curve, one press to the next.
    pub curve: Rect,
    pub phones_mix: KnobCell,
    pub phones_level: KnobCell,
}

fn step(m: &DensityMetrics, i: usize) -> i32 {
    m.space[i].round() as i32
}

/// Width of a knob's text block: room for "FILTER" in the label size and "+12.0" in mono.
pub fn knob_text_width(m: &DensityMetrics) -> i32 {
    (m.knob_size.round() as i32 + 4).max(34)
}

/// Width of an FX knob's text block: [`knob_text_width`], widened for a long parameter name
/// (estimated at 0.6 em a character in the label size) so "Feedback" does not run into the next knob.
pub fn fx_knob_text_width<'a>(
    m: &DensityMetrics,
    label_px: f32,
    labels: impl IntoIterator<Item = &'a str>,
) -> i32 {
    let widest = labels
        .into_iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let estimate = (widest as f32 * label_px * 0.6).ceil() as i32;
    knob_text_width(m).max(estimate)
}

/// Width of a channel fader: the cap is 0.85 of a knob wide (comp "Fader").
pub fn fader_width(m: &DensityMetrics) -> i32 {
    (m.knob_size * 0.85).round() as i32
}

/// Width of a stereo meter column: two 3 px bars and a 1 px gap.
pub const METER_WIDTH: i32 = 7;

/// Height of the knob columns: five knobs, `s2` apart.
pub fn knob_column_height(m: &DensityMetrics) -> i32 {
    KNOB_ROWS as i32 * m.knob_size.round() as i32 + (KNOB_ROWS as i32 - 1) * step(m, 1)
}

/// Height the mixer column needs: the knob columns, then the crossfader, the cue row and the
/// master row, `s3` apart.
pub fn column_height(m: &DensityMetrics) -> i32 {
    let h = m.control_height.round() as i32;
    knob_column_height(m) + 2 * h + m.knob_size.round() as i32 + 3 * step(m, 2)
}

/// Inner side padding of the mixer column.
pub fn column_padding(m: &DensityMetrics) -> i32 {
    step(m, 3)
}

/// A knob and its text in `row`: the knob on the inner side, the text on the outer one.
fn knob_cell(row: Rect, m: &DensityMetrics, text_on_left: bool) -> KnobCell {
    let k = m.knob_size.round() as i32;
    let mut row = row;
    let gap = step(m, 1);
    let knob = if text_on_left {
        row.remove_right(k)
    } else {
        row.remove_left(k)
    };
    if text_on_left {
        row.remove_right(gap);
    } else {
        row.remove_left(gap);
    }
    let text = if text_on_left {
        row.remove_right(knob_text_width(m))
    } else {
        row.remove_left(knob_text_width(m))
    };
    KnobCell { knob, text }
}

/// One channel's knob column, fader and meter in `top`, at its left edge (channel A) or right.
fn channel_top(top: Rect, m: &DensityMetrics, right: bool) -> (ChannelColumn, Rect) {
    let k = m.knob_size.round() as i32;
    let cell_w = k + step(m, 1) + knob_text_width(m);
    let mut rest = top;
    let mut column = if right {
        rest.remove_right(cell_w)
    } else {
        rest.remove_left(cell_w)
    };
    let knobs = std::array::from_fn(|_| {
        let row = column.remove_top(k);
        column.remove_top(step(m, 1));
        knob_cell(row, m, !right)
    });
    let take = |rest: &mut Rect, w: i32| {
        if right {
            rest.remove_right(w)
        } else {
            rest.remove_left(w)
        }
    };
    take(&mut rest, step(m, 2));
    let fader = take(&mut rest, fader_width(m));
    take(&mut rest, step(m, 0));
    let meter = take(&mut rest, METER_WIDTH);
    let channel = ChannelColumn {
        knobs,
        fader,
        meter,
        cue: Rect::default(),
        assign: Rect::default(),
    };
    (channel, rest)
}

/// The cue row: each channel's cue then assignment from its edge, the curve switch centred.
fn cue_row(row: Rect, m: &DensityMetrics, channels: &mut [ChannelColumn; 2]) -> Rect {
    let gap = step(m, 1);
    let unit = row.w / 6;
    let mut rest = row;
    channels[0].cue = rest.remove_left(unit);
    rest.remove_left(gap);
    channels[0].assign = rest.remove_left(unit);
    rest.remove_left(gap);
    channels[1].cue = rest.remove_right(unit);
    rest.remove_right(gap);
    channels[1].assign = rest.remove_right(unit);
    rest.remove_right(gap);
    rest
}

/// Lays out the mixer column in `bounds` (the mixer cell).
pub fn column_layout(bounds: Rect, m: &DensityMetrics) -> ColumnLayout {
    let h = m.control_height.round() as i32;
    let k = m.knob_size.round() as i32;
    let row_gap = step(m, 2);
    let mut area = bounds.reduced(column_padding(m), 0);
    let top = area.remove_top(knob_column_height(m));
    area.remove_top(row_gap);
    let (a, rest) = channel_top(top, m, false);
    let (b, rest) = channel_top(rest, m, true);
    let group_w = (METER_WIDTH + step(m, 0) + fader_width(m)).min(rest.w);
    let mut group = rest.with_size_keeping_centre(group_w, rest.h);
    let master_meter = group.remove_left(METER_WIDTH.min(group.w));
    group.remove_left(step(m, 0));
    let master_fader = group;
    let mut channels = [a, b];

    let mut fader_row = area.remove_top(h);
    area.remove_top(row_gap);
    let tag = h.min(fader_row.w / 4);
    let tag_a = fader_row.remove_left(tag);
    let tag_b = fader_row.remove_right(tag);
    let crossfader = fader_row.reduced(step(m, 2).min(fader_row.w / 2), 0);

    let curve = cue_row(area.remove_top(h), m, &mut channels);
    area.remove_top(row_gap);

    let knobs = area.remove_top(k);
    let cell_w = k + step(m, 1) + knob_text_width(m);
    let gap = step(m, 4);
    let mut pair = knobs.with_size_keeping_centre((2 * cell_w + gap).min(knobs.w), knobs.h);
    let phones_mix = knob_cell(pair.remove_left(cell_w), m, false);
    pair.remove_left(gap);
    let phones_level = knob_cell(pair, m, false);

    ColumnLayout {
        channels,
        master_fader,
        master_meter,
        tags: [tag_a, tag_b],
        crossfader,
        curve,
        phones_mix,
        phones_level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Density, ThemeTokens};

    fn column(density: Density) -> (DensityMetrics, Rect, ColumnLayout) {
        let t = ThemeTokens::builtin_dark();
        let m = *t.metrics(density);
        let bounds = Rect::new(600, 300, m.mixer_width as i32, column_height(&m));
        (m, bounds, column_layout(bounds, &m))
    }

    #[test]
    fn an_fx_knob_makes_room_for_a_long_name_and_keeps_the_column_width_for_short_ones() {
        let t = ThemeTokens::builtin_dark();
        for density in [Density::Roomy, Density::Compact] {
            let m = *t.metrics(density);
            let label = t.type_label;
            assert_eq!(
                fx_knob_text_width(&m, label, ["Mix", "Time"]),
                knob_text_width(&m)
            );
            let long = fx_knob_text_width(&m, label, ["Mix", "Feedback"]);
            assert!(long as f32 >= 8.0 * label * 0.55, "{density:?}: {long}");
        }
    }

    fn parts(l: &ColumnLayout) -> Vec<Rect> {
        let mut out = vec![
            l.master_fader,
            l.master_meter,
            l.tags[0],
            l.tags[1],
            l.crossfader,
            l.curve,
            l.phones_mix.knob,
            l.phones_mix.text,
            l.phones_level.knob,
            l.phones_level.text,
        ];
        for c in &l.channels {
            out.extend(c.knobs.iter().flat_map(|k| [k.knob, k.text]));
            out.extend([c.fader, c.meter, c.cue, c.assign]);
        }
        out
    }

    #[test]
    fn every_part_is_inside_the_column_and_none_overlap() {
        for density in [Density::Roomy, Density::Compact] {
            let (_, bounds, l) = column(density);
            let all = parts(&l);
            for (i, a) in all.iter().enumerate() {
                assert!(!a.is_empty(), "{density:?}: part {i} is empty");
                assert!(bounds.contains(a), "{density:?}: {a:?} escapes {bounds:?}");
                for b in &all[i + 1..] {
                    assert!(!a.intersects(b), "{density:?}: {a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn knobs_are_the_density_size_with_text_on_the_outer_side() {
        for density in [Density::Roomy, Density::Compact] {
            let (m, _, l) = column(density);
            let k = m.knob_size as i32;
            for cell in l.channels[0].knobs {
                assert_eq!((cell.knob.w, cell.knob.h), (k, k));
                assert!(
                    cell.text.right() <= cell.knob.x,
                    "A's text is left of its knob"
                );
            }
            for cell in l.channels[1].knobs {
                assert!(
                    cell.text.x >= cell.knob.right(),
                    "B's text is right of its knob"
                );
            }
            let [a, b] = &l.channels;
            assert!(a.fader.right() <= l.master_meter.x && l.master_fader.right() <= b.fader.x);
            assert_eq!(a.fader.h, b.fader.h);
        }
    }

    #[test]
    fn the_crossfader_runs_between_the_a_and_b_tags() {
        for density in [Density::Roomy, Density::Compact] {
            let (_, _, l) = column(density);
            assert!(l.tags[0].right() <= l.crossfader.x);
            assert!(l.crossfader.right() <= l.tags[1].x);
            assert_eq!(l.tags[0].y, l.crossfader.y);
            let [a, b] = &l.channels;
            assert!(a.assign.right() <= l.curve.x && l.curve.right() <= b.assign.x);
        }
    }
}
