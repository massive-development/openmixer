//! Geometry of a deck's stems drawer: one row per stem, each with a mute button carrying the stem's
//! name, a solo button, a volume slider and a filter slider with a centre detent. Rows are 32 px,
//! the smallest comfortable target. Geometry only; the gpui layer draws the rows and wires them to
//! [`crate::stem_binding::StemBinding`].

use super::deck::{STEM_ROW_GAP, STEM_ROW_HEIGHT};
use crate::geometry::Rect;

/// Rows per deck: one per stem.
pub const PADS: usize = 4;

/// The rectangles of one stem row, in the parent's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PadRects {
    /// Name button: toggles the mute.
    pub mute: Rect,
    pub solo: Rect,
    pub volume: Rect,
    /// Horizontal slider, centre = no filtering.
    pub filter: Rect,
}

/// Width of the mute (name) button and of the solo button, in px.
const MUTE_WIDTH: i32 = 60;
const SOLO_WIDTH: i32 = 32;

/// Places the four stem rows from the top of `bounds` (the stems drawer). `gap` separates the
/// buttons in a row. The sliders share what is left, volume 5 parts to filter 4.
pub fn stem_pad_layout(bounds: Rect, gap: i32) -> [PadRects; PADS] {
    std::array::from_fn(|i| {
        let y = bounds.y + i as i32 * (STEM_ROW_HEIGHT + STEM_ROW_GAP);
        let mut row = Rect::new(bounds.x, y, bounds.w, STEM_ROW_HEIGHT);
        let mute = row.remove_left(MUTE_WIDTH.min(row.w));
        row.remove_left(gap);
        let solo = row.remove_left(SOLO_WIDTH.min(row.w));
        row.remove_left(gap);
        let volume_w = ((row.w - gap) * 5 / 9).max(0);
        let volume = row.remove_left(volume_w);
        row.remove_left(gap);
        PadRects {
            mute,
            solo,
            volume,
            filter: row,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_stack_without_overlap_and_every_target_is_at_least_32_px_tall() {
        let bounds = Rect::new(0, 0, 198, 150);
        let rows = stem_pad_layout(bounds, 4);
        let mut parts = Vec::new();
        for row in &rows {
            for part in [row.mute, row.solo, row.volume, row.filter] {
                assert!(bounds.contains(&part), "{part:?} outside the drawer");
                assert!(part.h >= 32);
                parts.push(part);
            }
            assert!(row.filter.w >= 32, "the filter keeps a usable width");
        }
        for (i, a) in parts.iter().enumerate() {
            for b in &parts[i + 1..] {
                assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn rows_are_in_stem_order_top_to_bottom() {
        let rows = stem_pad_layout(Rect::new(0, 0, 300, 150), 4);
        assert!(rows.windows(2).all(|w| w[0].mute.y < w[1].mute.y));
    }
}
