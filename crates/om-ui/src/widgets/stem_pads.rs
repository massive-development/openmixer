//! Geometry of a deck's stem pads row: a control column (the 'Load stems' / 'Stems off' button)
//! and one pad per stem. Each pad has a header (the mute tap target, with the solo button at its
//! right), a vertical volume slider and a filter knob below. Geometry only; the gpui layer draws
//! the pads and wires them to [`crate::stem_binding::StemBinding`].

use super::px;
use crate::geometry::Rect;
use crate::theme::ThemeTokens;

/// Pads per deck: one per stem.
pub const PADS: usize = 4;

/// The rectangles of one stem pad, in the parent's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PadRects {
    /// Tap target and label: toggles the mute.
    pub mute: Rect,
    /// Solo button, at the header's right end.
    pub solo: Rect,
    pub volume: Rect,
    pub filter: Rect,
}

/// The stems row: its control column and the four pads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StemPadLayout {
    pub control: Rect,
    pub pads: [PadRects; PADS],
}

/// Height of the stems row: a header row, a gap, and a body of about one and a half rows for the
/// slider and knob (the deck card is short at 900x600, so this is the least that stays usable).
pub fn stems_row_height(theme: &ThemeTokens) -> i32 {
    px(theme.deck_row_height) * 5 / 2 + theme.inner_gap
}

/// Places the control column and the pads inside `bounds` (the stems row).
pub fn stem_pad_layout(bounds: Rect, theme: &ThemeTokens) -> StemPadLayout {
    let gap = theme.inner_gap;
    let row = px(theme.deck_row_height);
    let mut area = bounds;
    let control = area.remove_left(row * 2 + row / 8);
    area.remove_left(gap);
    let cell_w = ((area.w - gap * (PADS as i32 - 1)) / PADS as i32).max(0);
    let mut pads = [PadRects::default(); PADS];
    for (index, pad) in pads.iter_mut().enumerate() {
        let x = area.x + index as i32 * (cell_w + gap);
        let mut column = Rect::new(x, area.y, cell_w, area.h);
        let header = column.remove_top(row);
        column.remove_top(gap);
        // The solo button is narrower than a row: on a deck beside a channel strip the pads share
        // about 230 px, and the stem name needs the rest.
        let solo_w = (row * 4 / 5).min(header.w);
        pad.mute = Rect::new(header.x, header.y, (header.w - solo_w).max(0), header.h);
        pad.solo = Rect::new(
            header.x + (header.w - solo_w).max(0),
            header.y,
            solo_w,
            header.h,
        );
        let slider_w = column.w * 45 / 100;
        pad.volume = Rect::new(column.x, column.y, slider_w, column.h);
        pad.filter = Rect::new(
            column.x + slider_w + gap / 2,
            column.y,
            (column.w - slider_w - gap / 2).max(0),
            column.h,
        );
    }
    StemPadLayout { control, pads }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(outer: &Rect, inner: &Rect) -> bool {
        inner.w >= 0
            && inner.h >= 0
            && inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.w <= outer.x + outer.w
            && inner.y + inner.h <= outer.y + outer.h
    }

    #[test]
    fn every_pad_part_sits_inside_the_row_and_the_parts_do_not_overlap() {
        let theme = ThemeTokens::builtin_dark();
        let row = Rect::new(0, 0, 360, stems_row_height(&theme));
        let layout = stem_pad_layout(row, &theme);
        assert!(inside(&row, &layout.control));
        let mut parts = vec![layout.control];
        for pad in &layout.pads {
            for part in [pad.mute, pad.solo, pad.volume, pad.filter] {
                assert!(inside(&row, &part), "{part:?} outside the row");
                parts.push(part);
            }
        }
        for (i, a) in parts.iter().enumerate() {
            for b in &parts[i + 1..] {
                if a.w > 0 && a.h > 0 && b.w > 0 && b.h > 0 {
                    assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn pads_are_in_stem_order_left_to_right() {
        let theme = ThemeTokens::builtin_dark();
        let row = Rect::new(0, 0, 500, stems_row_height(&theme));
        let layout = stem_pad_layout(row, &theme);
        let xs: Vec<i32> = layout.pads.iter().map(|p| p.mute.x).collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]));
    }
}
