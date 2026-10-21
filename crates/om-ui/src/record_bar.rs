//! The header's Record button as a view-model: what the button shows (red dot while recording,
//! elapsed `mm:ss`, a dropped-frames warning, a mini peak meter) and where it sits in the header
//! strip. Pure: no drawing and no service calls. The window draws it with `LookAndFeel` colours and
//! sends `record.toggle` on a click or the `r` key.

use crate::geometry::Rect;
use om_core::RecordSnapshot;

/// Width of the Record button, in pixels.
pub const RECORD_WIDTH: i32 = 200;
/// Gap between the Record button and its neighbours.
pub const RECORD_GAP: i32 = 8;

/// What the button shows.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordBar {
    pub recording: bool,
    /// The writer stopped itself (disk full or an i/o error).
    pub failed: bool,
    /// `Record`, `00:00` style elapsed time while recording, or `Stopped` after a failure.
    pub label: String,
    /// `12.3 MB` written so far (MB = 1 000 000 bytes), `0.0 MB` before the first block.
    pub size: String,
    /// `12 frames dropped`; empty when none were.
    pub warning: String,
    /// Peaks, clamped to `0..=1`.
    pub peak_left: f32,
    pub peak_right: f32,
    /// Shown in a tooltip or the notice line; empty when none.
    pub error: String,
}

impl RecordBar {
    pub fn new(snapshot: &RecordSnapshot, error: &str) -> Self {
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let label = if snapshot.failed {
            "Stopped".to_owned()
        } else if snapshot.recording {
            elapsed_text(snapshot.elapsed_s)
        } else {
            "Record".to_owned()
        };
        Self {
            recording: snapshot.recording,
            failed: snapshot.failed,
            label,
            size: size_text(snapshot.bytes),
            warning: dropped_text(snapshot.dropped_frames),
            peak_left: clamp(snapshot.peak_left),
            peak_right: clamp(snapshot.peak_right),
            error: error.to_owned(),
        }
    }
}

/// `mm:ss` (`h:mm:ss` from one hour); negative and non-finite read as zero.
pub fn elapsed_text(seconds: f64) -> String {
    let total = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// `12.3 MB`: bytes written, in decimal megabytes.
pub fn size_text(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// `1 frame dropped`, `12 frames dropped`, empty for zero.
pub fn dropped_text(frames: u64) -> String {
    match frames {
        0 => String::new(),
        1 => "1 frame dropped".to_owned(),
        n => format!("{n} frames dropped"),
    }
}

/// Where the button sits in a header strip of `window_width`, left of a Settings button that
/// reserves `settings_reserve` pixels at the right edge. `None` when the strip is too narrow.
pub fn record_rect(
    window_width: i32,
    header_top: i32,
    header_height: i32,
    settings_reserve: i32,
) -> Option<Rect> {
    let x = window_width - settings_reserve - RECORD_GAP - RECORD_WIDTH;
    (x >= 0).then(|| Rect::new(x, header_top, RECORD_WIDTH, header_height))
}

/// The width left to the automix bar once the Record button has its place.
pub fn automix_width(window_width: i32, settings_reserve: i32) -> i32 {
    (window_width - settings_reserve - RECORD_WIDTH - 2 * RECORD_GAP).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_state() {
        let idle = RecordBar::new(&RecordSnapshot::default(), "");
        assert_eq!((idle.label.as_str(), idle.recording), ("Record", false));
        let rec = RecordBar::new(
            &RecordSnapshot {
                recording: true,
                elapsed_s: 83.9,
                dropped_frames: 12,
                ..Default::default()
            },
            "",
        );
        assert_eq!(rec.label, "01:23");
        assert_eq!(rec.warning, "12 frames dropped");
        let failed = RecordBar::new(
            &RecordSnapshot {
                failed: true,
                ..Default::default()
            },
            "disk full",
        );
        assert_eq!(
            (failed.label.as_str(), failed.error.as_str()),
            ("Stopped", "disk full")
        );
    }

    #[test]
    fn hostile_snapshot_values_stay_in_range() {
        let bar = RecordBar::new(
            &RecordSnapshot {
                recording: true,
                elapsed_s: f64::NAN,
                peak_left: f32::INFINITY,
                peak_right: -3.0,
                ..Default::default()
            },
            "",
        );
        assert_eq!(
            (bar.label.as_str(), bar.peak_left, bar.peak_right),
            ("00:00", 0.0, 0.0)
        );
        assert_eq!(elapsed_text(3_661.0), "1:01:01");
        assert_eq!(size_text(12_340_000), "12.3 MB");
        assert_eq!(size_text(0), "0.0 MB");
        assert_eq!(dropped_text(1), "1 frame dropped");
    }

    /// The button never overlaps the Settings button or the (shrunk) automix bar, at the sizes the
    /// window is tested at. The deck area starts below the header, so MainLayout is unaffected.
    #[test]
    fn the_button_fits_the_header_at_the_test_sizes() {
        const SETTINGS_RESERVE: i32 = 96;
        for (w, h) in [(900, 600), (1024, 640), (1280, 800), (1920, 1080)] {
            let record = record_rect(w, 0, 32, SETTINGS_RESERVE).expect("fits");
            let automix = Rect::new(0, 0, automix_width(w, SETTINGS_RESERVE), 32);
            let settings = Rect::new(w - SETTINGS_RESERVE, 0, SETTINGS_RESERVE, 32);
            for (a, b) in [(record, automix), (record, settings), (automix, settings)] {
                assert!(
                    a.x + a.w <= b.x || b.x + b.w <= a.x,
                    "{w}x{h}: {a:?} vs {b:?}"
                );
            }
            assert!(record.x + record.w <= w);
            assert!(automix.w > 300, "{w}x{h}: automix keeps room");
        }
        assert!(record_rect(200, 0, 32, 96).is_none());
    }
}
