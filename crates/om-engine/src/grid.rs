//! Beat grid seen by sync and quantize. Analysis supplies real grids (Wave 3); [`FixedTempoGrid`]
//! covers constant-tempo material and tests. Positions are source frames; beat 0 is a downbeat.
//! Port of `src/engine/BeatGrid.h`.

/// Maps source frames to beats and back. Implementations are shared between control threads.
pub trait BeatGrid: Send + Sync {
    fn bpm(&self) -> f64;

    fn beats_per_bar(&self) -> i32 {
        4
    }

    fn beat_at_frame(&self, frame: i64) -> f64;

    fn frame_at_beat(&self, beat: f64) -> i64;
}

/// Constant-tempo grid. `first_beat_frame` is the source frame of beat 0 (may be negative or
/// beyond the start of the track).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedTempoGrid {
    tempo: f64,
    frames_per_beat: f64,
    origin: i64,
}

impl FixedTempoGrid {
    pub fn new(bpm: f64, source_sample_rate: f64, first_beat_frame: i64) -> Self {
        Self {
            tempo: bpm,
            frames_per_beat: source_sample_rate * 60.0 / bpm,
            origin: first_beat_frame,
        }
    }
}

impl BeatGrid for FixedTempoGrid {
    fn bpm(&self) -> f64 {
        self.tempo
    }

    fn beat_at_frame(&self, frame: i64) -> f64 {
        (frame - self.origin) as f64 / self.frames_per_beat
    }

    fn frame_at_beat(&self, beat: f64) -> i64 {
        let offset = if beat < 0.0 { -0.5 } else { 0.5 };
        self.origin + (beat * self.frames_per_beat + offset) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;

    #[test]
    fn fixed_grid_maps_frames_and_beats() {
        let grid = FixedTempoGrid::new(120.0, RATE, 1000);
        assert_eq!(grid.frame_at_beat(2.0), 1000 + 44_100);
        assert!((grid.beat_at_frame(1000 + 22_050) - 1.0).abs() < 1e-9);
        assert!((grid.beat_at_frame(0) - (-1000.0 / 22_050.0)).abs() < 1e-9);
        assert_eq!(grid.beats_per_bar(), 4);
    }
}
