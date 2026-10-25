//! The header's Record button as a view-model: what the button shows (red dot while recording,
//! elapsed `mm:ss`, a dropped-frames warning, a mini peak meter) and where it sits in the header
//! strip. Pure: no drawing and no service calls. The window draws it with `LookAndFeel` colours and
//! sends `record.toggle` on a click or the `r` key.

use crate::geometry::Rect;
use om_core::RecordSnapshot;

pub use crate::header_layout::RECORD_WIDTH;

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

/// Where the button sits in a header strip of `window_width` with `header_height` px, left of a
/// Settings button that reserves `settings_reserve` pixels (0 for none) at the right edge, at
/// `header_top`. A thin wrapper over [`crate::header_layout::header_rects`]; `None` when the strip is
/// too narrow.
pub fn record_rect(
    window_width: i32,
    header_top: i32,
    header_height: i32,
    settings_reserve: i32,
) -> Option<Rect> {
    let rects = crate::header_layout::header_rects(
        window_width,
        header_height,
        crate::header_layout::HeaderNeeds {
            automix: true,
            record: true,
            settings: settings_reserve > 0,
            notice: false,
        },
    );
    let at = rects.record?;
    (at.x >= rects.brand.right()).then(|| Rect::new(at.x, header_top + at.y, at.w, at.h))
}

/// The width left to the automix bar when the strip has a Record button and a Settings button.
pub fn automix_width(window_width: i32, settings_reserve: i32) -> i32 {
    let rects = crate::header_layout::header_rects(
        window_width,
        crate::main_layout::HEADER_HEIGHT_SHORT,
        crate::header_layout::HeaderNeeds {
            automix: true,
            record: true,
            settings: settings_reserve > 0,
            notice: false,
        },
    );
    rects.automix.map_or(0, |r| r.w)
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

    /// The wrappers agree with the header layout: the button sits right of the automix bar, left
    /// of the toggle and Settings, and a strip too narrow for it has none.
    #[test]
    fn the_button_sits_right_of_the_automix_bar() {
        for (w, h) in [(1024, 640), (1280, 800), (1920, 1080)] {
            let header_h = crate::main_layout::header_height(h);
            let record = record_rect(w, 0, header_h, 96).expect("fits");
            assert!(record.x >= automix_width(w, 96) && record.right() <= w - 96);
            assert_eq!(record.w, RECORD_WIDTH);
        }
        assert!(record_rect(200, 0, 36, 96).is_none());
    }
}
