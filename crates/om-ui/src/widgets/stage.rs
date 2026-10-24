//! The waveform stage: the part of the window a DJ watches. Each deck's header and overview share
//! the top band side by side; the zoomed waveforms sit under them, full width and stacked, so both
//! tracks' beat grids line up on one screen. Geometry only; drawing is the gpui layer's job.

use super::deck::DeckLayout;
use crate::geometry::Rect;
use crate::theme::ThemeTokens;

/// Decks the stage draws. Further decks keep a full card with their own zoomed waveform.
pub const STAGE_DECKS: usize = 2;

/// Header (title and artist lines), overview strip and smallest readable zoomed strip on the stage,
/// in px. Tighter than a deck card's: the stage shares the window height with the controls.
pub const STAGE_HEADER_HEIGHT: i32 = 46;
pub const STAGE_OVERVIEW_HEIGHT: i32 = 48;
pub const STAGE_MIN_ZOOM_HEIGHT: i32 = 32;

/// Places the header, overview and zoomed waveform of each stage deck inside `bounds` (the stage
/// cell, origin at 0,0). Only those three rectangles of each [`DeckLayout`] are set; the controls
/// live in the deck cards below. The zoomed strips are hidden when they would be shorter than the
/// minimum ([`STAGE_MIN_ZOOM_HEIGHT`]).
pub fn stage_layout(bounds: Rect, theme: &ThemeTokens, decks: usize) -> Vec<DeckLayout> {
    let decks = decks.min(STAGE_DECKS);
    if decks == 0 {
        return Vec::new();
    }
    let gap = theme.inner_gap;
    let mut area = bounds.reduced(theme.pad, theme.pad);
    let n = decks as i32;
    let header_h = STAGE_HEADER_HEIGHT;
    // On a short stage the overview strips go first, so the zoomed waveforms stay readable.
    let zoom_h_with = |overview_h: i32| ((area.h - header_h - overview_h - gap * n) / n).max(0);
    let overview_h = if zoom_h_with(STAGE_OVERVIEW_HEIGHT) >= STAGE_MIN_ZOOM_HEIGHT {
        STAGE_OVERVIEW_HEIGHT
    } else {
        0
    };
    let mut top = area.remove_top(header_h + overview_h);
    area.remove_top(gap);
    let zoom_h = ((area.h - gap * (n - 1)) / n).max(0);
    let zoom_visible = zoom_h >= STAGE_MIN_ZOOM_HEIGHT;
    let column_w = ((top.w - theme.gap * (n - 1)) / n).max(0);

    (0..decks)
        .map(|i| {
            let mut column = if i + 1 == decks {
                top
            } else {
                let c = top.remove_left(column_w);
                top.remove_left(theme.gap);
                c
            };
            let header = column.remove_top(header_h);
            let overview = column;
            let zoom = Rect::new(area.x, area.y + (zoom_h + gap) * i as i32, area.w, zoom_h);
            DeckLayout {
                header,
                overview,
                zoom: if zoom_visible { zoom } else { Rect::default() },
                zoom_visible,
                ..DeckLayout::empty()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ThemeTokens {
        ThemeTokens::builtin_dark()
    }

    #[test]
    fn two_decks_share_the_top_band_and_stack_full_width_waveforms() {
        let t = theme();
        let bounds = Rect::sized(1200, 330);
        let l = stage_layout(bounds, &t, 2);
        assert_eq!(l.len(), 2);
        assert!(
            l[0].header.right() < l[1].header.x,
            "headers sit side by side"
        );
        assert_eq!(l[0].header.y, l[1].header.y);
        assert!(l[0].overview.y >= l[0].header.bottom());
        assert!(l[0].zoom.bottom() <= l[1].zoom.y, "the strips stack");
        assert_eq!(l[0].zoom.w, l[1].zoom.w);
        assert!(l[0].zoom.w > 1100, "full width: {:?}", l[0].zoom);
        assert!(l[0].zoom.y >= l[0].overview.bottom());
        for d in &l {
            assert!(d.zoom_visible);
            assert!(bounds.contains(&d.header) && bounds.contains(&d.overview));
            assert!(bounds.contains(&d.zoom));
        }
    }

    #[test]
    fn one_deck_takes_the_whole_band_and_extra_decks_are_ignored() {
        let t = theme();
        let one = stage_layout(Rect::sized(1000, 300), &t, 1);
        assert_eq!(one.len(), 1);
        assert!(one[0].header.w > 900);
        assert_eq!(
            stage_layout(Rect::sized(1000, 300), &t, 4).len(),
            STAGE_DECKS
        );
        assert!(stage_layout(Rect::sized(1000, 300), &t, 0).is_empty());
    }

    #[test]
    fn a_short_stage_drops_the_overviews_before_the_zoomed_strips() {
        let t = theme();
        let l = stage_layout(Rect::sized(1000, 170), &t, 2);
        assert!(l.iter().all(|d| d.zoom_visible && d.overview.is_empty()));
        assert!(l.iter().all(|d| d.zoom.h >= STAGE_MIN_ZOOM_HEIGHT));
    }

    #[test]
    fn a_very_short_stage_hides_the_zoomed_strips_instead_of_squashing_them() {
        let t = theme();
        let l = stage_layout(Rect::sized(1000, 120), &t, 2);
        assert!(l.iter().all(|d| !d.zoom_visible && d.zoom.is_empty()));
    }
}
