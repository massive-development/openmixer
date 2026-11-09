//! The waveform stage: the part of the window a DJ watches. Deck A's and deck B's zoomed waveforms
//! are stacked under the deck header row, full width and [`WAVE_GAP`] apart, so both tracks' beat
//! grids line up on one screen (comp 4a/4b). Geometry only; drawing is the gpui layer's job.

use crate::geometry::Rect;
use crate::main_layout::{FACEPLATE_DECKS, MIN_WAVE_HEIGHT, WAVE_GAP};

/// Decks the stage draws.
pub const STAGE_DECKS: usize = FACEPLATE_DECKS;

/// The zoomed waveform of each stage deck inside `bounds` (the stage cell), top to bottom. A strip
/// shorter than [`MIN_WAVE_HEIGHT`] is not drawn: the list is empty rather than squashed.
pub fn stage_layout(bounds: Rect, decks: usize) -> Vec<Rect> {
    let n = decks.min(STAGE_DECKS) as i32;
    if n == 0 {
        return Vec::new();
    }
    let h = (bounds.h - WAVE_GAP * (n - 1)) / n;
    if h < MIN_WAVE_HEIGHT {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            let y = bounds.y + (h + WAVE_GAP) * i;
            let last = i + 1 == n;
            Rect::new(
                bounds.x,
                y,
                bounds.w,
                if last { bounds.bottom() - y } else { h },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_decks_stack_full_width_strips_two_px_apart() {
        let bounds = Rect::new(21, 140, 1398, 300);
        let l = stage_layout(bounds, 2);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].w, bounds.w);
        assert_eq!(l[1].y - l[0].bottom(), WAVE_GAP);
        assert_eq!(l[1].bottom(), bounds.bottom());
        assert!((l[0].h - l[1].h).abs() <= 1);
    }

    #[test]
    fn one_deck_takes_the_stage_and_extra_decks_are_ignored() {
        let bounds = Rect::sized(1000, 300);
        assert_eq!(stage_layout(bounds, 1), vec![bounds]);
        assert_eq!(stage_layout(bounds, 4).len(), STAGE_DECKS);
        assert!(stage_layout(bounds, 0).is_empty());
    }

    #[test]
    fn a_very_short_stage_hides_the_strips_instead_of_squashing_them() {
        assert!(stage_layout(Rect::sized(1000, 60), 2).is_empty());
        assert_eq!(stage_layout(Rect::sized(1000, 66), 2).len(), 2);
    }
}
