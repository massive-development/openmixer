//! The size the main window opens at: 1920x1080, shrunk to fit the display, never below the layout
//! minimum. `OPENMIXER_WINDOW=WIDTHxHEIGHT` overrides it (the screenshot script pins 1280x800).

use om_ui::main_layout::MIN_WINDOW;

/// Full HD, the default on a display that has room for it.
pub const DEFAULT_WINDOW: (f32, f32) = (1920.0, 1080.0);
/// Share of the display the default may take, so the window frame and taskbar stay visible.
const DISPLAY_SHARE: f32 = 0.9;

/// Parses `1280x800` (also `1280X800`); `None` for anything else or a size below the layout minimum.
pub fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (w, h) = text.trim().split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?);
    (w >= MIN_WINDOW.0 as f32 && h >= MIN_WINDOW.1 as f32).then_some((w, h))
}

/// An explicit `requested` size wins. Otherwise the default, clamped to `DISPLAY_SHARE` of
/// `display` (width, height) when known, and never below the layout minimum.
pub fn initial_size(requested: Option<(f32, f32)>, display: Option<(f32, f32)>) -> (f32, f32) {
    if let Some(size) = requested {
        return size;
    }
    let (min_w, min_h) = (MIN_WINDOW.0 as f32, MIN_WINDOW.1 as f32);
    let (w, h) = DEFAULT_WINDOW;
    match display {
        Some((dw, dh)) => (
            w.min(dw * DISPLAY_SHARE).max(min_w),
            h.min(dh * DISPLAY_SHARE).max(min_h),
        ),
        None => (w, h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_full_hd_when_the_display_has_room() {
        assert_eq!(initial_size(None, Some((2560.0, 1440.0))), (1920.0, 1080.0));
        assert_eq!(initial_size(None, None), (1920.0, 1080.0));
    }

    #[test]
    fn a_small_display_shrinks_the_default_but_not_below_the_minimum() {
        assert_eq!(initial_size(None, Some((1440.0, 900.0))), (1296.0, 810.0));
        let (w, h) = initial_size(None, Some((1024.0, 600.0)));
        assert_eq!((w, h), (1024.0, 640.0));
    }

    #[test]
    fn an_explicit_size_wins_and_parsing_rejects_bad_input() {
        assert_eq!(
            initial_size(Some((1280.0, 800.0)), Some((800.0, 600.0))),
            (1280.0, 800.0)
        );
        assert_eq!(parse_size("1280x800"), Some((1280.0, 800.0)));
        assert_eq!(parse_size(" 1600X900 "), Some((1600.0, 900.0)));
        assert_eq!(parse_size("640x480"), None, "below the layout minimum");
        assert_eq!(parse_size("wide"), None);
        assert_eq!(parse_size("1280x"), None);
    }
}
