//! Snaps positions to a beat grid for quantized cue and loop. Pure functions, any thread.
//! Without a grid or with a division of 0 positions pass through unchanged. Port of
//! `src/engine/Quantizer.*`.

use crate::grid::BeatGrid;

/// Loop bounds in source frames. Valid when `end > start`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Loop {
    pub start: i64,
    pub end: i64,
}

impl Loop {
    pub fn valid(&self) -> bool {
        self.end > self.start
    }
}

fn snappable(grid: Option<&dyn BeatGrid>, division: f64) -> Option<&dyn BeatGrid> {
    grid.filter(|g| division > 0.0 && g.bpm() > 0.0)
}

/// Nearest grid step of `division` beats (1 = beat, 0.5 = half beat, 4 = bar).
pub fn snap_to_nearest(grid: Option<&dyn BeatGrid>, frame: i64, division: f64) -> i64 {
    match snappable(grid, division) {
        Some(g) => {
            let beat = (g.beat_at_frame(frame) / division).round() * division;
            g.frame_at_beat(beat)
        }
        None => frame,
    }
}

/// Grid step of `division` beats at or before `frame`.
pub fn snap_to_previous(grid: Option<&dyn BeatGrid>, frame: i64, division: f64) -> i64 {
    match snappable(grid, division) {
        Some(g) => {
            let beat = (g.beat_at_frame(frame) / division).floor() * division;
            g.frame_at_beat(beat)
        }
        None => frame,
    }
}

/// Loop of `length_beats` starting on the grid step nearest to `start_frame`. Invalid when there
/// is no grid or the length is not positive.
pub fn make_quantized_loop(
    grid: Option<&dyn BeatGrid>,
    start_frame: i64,
    length_beats: f64,
    division: f64,
) -> Loop {
    let Some(grid) = grid else {
        return Loop::default();
    };
    if length_beats <= 0.0 {
        return Loop::default();
    }
    let start = snap_to_nearest(Some(grid), start_frame, division);
    Loop {
        start,
        end: grid.frame_at_beat(grid.beat_at_frame(start) + length_beats),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::FixedTempoGrid;

    const RATE: f64 = 44_100.0;

    #[test]
    fn snaps_to_beats_fractions_and_bars_and_passes_through_without_a_grid() {
        let grid = FixedTempoGrid::new(120.0, RATE, 1000);
        let g: Option<&dyn BeatGrid> = Some(&grid);
        assert_eq!(snap_to_nearest(g, 1000 + 22_050 + 3000, 1.0), 1000 + 22_050);
        assert_eq!(snap_to_nearest(g, 1000 + 22_050 + 12_000, 1.0), 1000 + 44_100);
        assert_eq!(snap_to_previous(g, 1000 + 22_050 + 12_000, 1.0), 1000 + 22_050);
        assert_eq!(snap_to_nearest(g, 1000 + 6000, 0.25), 1000 + 5512 + 1); // 1/4 beat = 5512.5
        assert_eq!(snap_to_previous(g, 1000 + 80_000, 4.0), 1000);
        assert_eq!(snap_to_nearest(None, 12_345, 1.0), 12_345);
        assert_eq!(snap_to_nearest(g, 12_345, 0.0), 12_345);
    }

    #[test]
    fn quantized_loop_starts_on_the_grid_and_spans_the_requested_beats() {
        let grid = FixedTempoGrid::new(120.0, RATE, 1000);
        let g: Option<&dyn BeatGrid> = Some(&grid);
        let loop_ = make_quantized_loop(g, 1000 + 22_050 + 900, 4.0, 1.0);
        assert!(loop_.valid());
        assert_eq!(loop_.start, 1000 + 22_050);
        assert_eq!(loop_.end - loop_.start, 4 * 22_050);
        assert!(!make_quantized_loop(g, 0, 0.0, 1.0).valid());
        assert!(!make_quantized_loop(None, 0, 4.0, 1.0).valid());
    }
}
