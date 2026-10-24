//! Discrete zoom for the zoomed strip, expressed as seconds of track across the strip width, so
//! the same step shows the same musical span at any window size and on any deck. Pure.

/// Seconds across the strip at each zoom step, widest last.
pub const ZOOM_SECONDS: [f64; 4] = [4.0, 8.0, 16.0, 32.0];
/// Step a deck starts at (8 s).
pub const DEFAULT_ZOOM_STEP: usize = 1;

/// Clamps a step index into [`ZOOM_SECONDS`].
pub fn clamp_step(step: usize) -> usize {
    step.min(ZOOM_SECONDS.len() - 1)
}

/// The step one wheel notch away: scrolling down (`delta_y > 0`) zooms in (fewer seconds), up
/// zooms out. Stays put at either end.
pub fn step_after_wheel(step: usize, delta_y: f32) -> usize {
    let step = clamp_step(step);
    if delta_y > 0.0 {
        step.saturating_sub(1)
    } else if delta_y < 0.0 {
        clamp_step(step + 1)
    } else {
        step
    }
}

/// Seconds of track across the strip at `step`.
pub fn seconds_at(step: usize) -> f64 {
    ZOOM_SECONDS[clamp_step(step)]
}

/// Frames per pixel that fit `seconds` of audio at `sample_rate` into `width_px`. Zero width or
/// rate gives 1 so a caller never divides by zero.
pub fn frames_per_pixel(seconds: f64, sample_rate: f64, width_px: f64) -> f64 {
    if width_px > 0.0 && sample_rate > 0.0 {
        seconds * sample_rate / width_px
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_steps_double_each_time() {
        assert_eq!(ZOOM_SECONDS, [4.0, 8.0, 16.0, 32.0]);
        assert_eq!(seconds_at(DEFAULT_ZOOM_STEP), 8.0);
        assert_eq!(seconds_at(99), 32.0, "out of range clamps");
    }

    #[test]
    fn wheel_steps_one_notch_and_stops_at_the_ends() {
        assert_eq!(step_after_wheel(1, 1.0), 0, "down zooms in");
        assert_eq!(step_after_wheel(0, 1.0), 0);
        assert_eq!(step_after_wheel(1, -1.0), 2, "up zooms out");
        assert_eq!(step_after_wheel(3, -1.0), 3);
        assert_eq!(step_after_wheel(2, 0.0), 2);
    }

    #[test]
    fn the_same_step_spans_the_same_time_at_any_width() {
        // 8 s at 44.1 kHz over 1000 px is 352.8 frames per pixel.
        assert!((frames_per_pixel(8.0, 44_100.0, 1000.0) - 352.8).abs() < 1e-9);
        let wide = frames_per_pixel(8.0, 44_100.0, 2000.0);
        assert!((wide * 2000.0 - 8.0 * 44_100.0).abs() < 1e-6);
        assert_eq!(frames_per_pixel(8.0, 44_100.0, 0.0), 1.0);
        assert_eq!(frames_per_pixel(8.0, 0.0, 100.0), 1.0);
    }
}
