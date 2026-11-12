//! Where the header strip's parts sit (comp 4a/4b): the wordmark at the left, the automix bar after
//! it, then from the right edge Settings, the library toggle, Record, the loudness toggle and the
//! latency and dropout indicator. Pure geometry shared by the window and the page layout. The strip keeps `s4` above
//! its parts and `s3` plus a 1 px etched line under them; every part is one control high on one
//! top, so the controls share a baseline; the parts never overlap and run left to right in a fixed
//! order. The master tempo is not here: it sits over the mixer in the deck header row.

use crate::geometry::Rect;
use crate::theme::DensityMetrics;

/// Width of the OPENMIXER wordmark: Sansation Bold 21 px measures 124 px (Phase 2 spike, openmixer-workbench/docs/spikes/DECISIONS.md).
pub const BRAND_WIDTH: i32 = 128;
/// Width of the notice chip (two lines of caption text).
pub const NOTICE_WIDTH: i32 = 240;
/// Width of the notice chip collapsed to the word "Notice" and the dismiss button.
pub const COLLAPSED_NOTICE_WIDTH: i32 = 72;
/// The automix bar's width the plan wants at 1280 wide; a notice chip collapses before the bar
/// goes under it.
pub const AUTOMIX_FLOOR: i32 = 300;
/// Width of the latency and dropout indicator: "123 ms  12 dropouts" in mono 13 px is 124 px.
pub const LATENCY_WIDTH: i32 = 128;
/// Width of the loudness toggle: the LUFS button and the momentary reading beside it.
pub const LOUDNESS_WIDTH: i32 = 112;
/// Width of the Record button.
pub const RECORD_WIDTH: i32 = 148;
/// Width of the library toggle (was PERFORM).
pub const LIBRARY_TOGGLE_WIDTH: i32 = 100;
/// Width of the Settings button.
pub const SETTINGS_WIDTH: i32 = 96;
/// The automix bar needs at least this much to show its segmented switch and the next track.
pub const MIN_AUTOMIX_WIDTH: i32 = 200;

/// Which optional parts the window has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeaderNeeds {
    pub automix: bool,
    pub record: bool,
    pub settings: bool,
    pub notice: bool,
}

/// The header's parts, in window pixels. `None` means the window has no such part or is too narrow
/// to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderRects {
    /// The whole strip: `x = 0`, `y = 0`, the window's width and [`header_height`].
    pub bar: Rect,
    /// The OPENMIXER wordmark.
    pub brand: Rect,
    /// Segmented Off|On, the transition length and the next track.
    pub automix: Option<Rect>,
    /// A dismissible chip that replaces the old banner row.
    pub notice: Option<Rect>,
    /// Output latency and dropouts.
    pub latency: Rect,
    /// The LUFS toggle and the momentary reading; its panel opens under it.
    pub loudness: Rect,
    pub record: Option<Rect>,
    /// Shows or hides the library band (was PERFORM).
    pub library_toggle: Rect,
    pub settings: Option<Rect>,
}

impl HeaderRects {
    /// Every part that is present, left to right.
    pub fn all(&self) -> Vec<Rect> {
        [
            Some(self.brand),
            self.automix,
            self.notice,
            Some(self.latency),
            Some(self.loudness),
            self.record,
            Some(self.library_toggle),
            self.settings,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// The strip's height for a metric set: `s4` above one control height, `s3` under it, and the
/// 1 px etched line.
pub fn header_height(m: &DensityMetrics) -> i32 {
    let s = |i: usize| m.space[i].round() as i32;
    s(3) + m.control_height.round() as i32 + s(2) + 1
}

/// Lays the strip out for a window `window_w` wide in metric set `m`.
pub fn header_rects(window_w: i32, m: &DensityMetrics, needs: HeaderNeeds) -> HeaderRects {
    let (top, h) = (m.space[3].round() as i32, m.control_height.round() as i32);
    let margin = m.space[4].round() as i32;
    let gap = m.space[3].round() as i32;
    let part = |x: i32, w: i32| Rect::new(x, top, w, h);

    // The right cluster is anchored to the window's right edge.
    let mut edge = window_w - margin;
    let mut take = |w: i32, wanted: bool| -> Option<Rect> {
        if !wanted {
            return None;
        }
        let rect = part(edge - w, w);
        edge = rect.x - gap;
        Some(rect)
    };
    let settings = take(SETTINGS_WIDTH, needs.settings);
    let library_toggle = take(LIBRARY_TOGGLE_WIDTH, true).unwrap_or_default();
    let record = take(RECORD_WIDTH, needs.record);
    let loudness = take(LOUDNESS_WIDTH, true).unwrap_or_default();
    let latency = take(LATENCY_WIDTH, true).unwrap_or_default();

    let brand = part(margin, BRAND_WIDTH);
    let free_start = brand.right() + gap;
    let free_end = latency.x - gap;
    // The chip keeps the automix bar at or above its floor: it takes its full width only when that
    // leaves the bar enough, and collapses to the word and its dismiss button if not.
    let notice = needs
        .notice
        .then(|| {
            let room = free_end - free_start - gap - AUTOMIX_FLOOR;
            let w = if !needs.automix || room >= NOTICE_WIDTH {
                NOTICE_WIDTH
            } else {
                COLLAPSED_NOTICE_WIDTH
            };
            let w = w.min((free_end - free_start).max(0));
            part(free_end - w, w)
        })
        .filter(|r| r.w > 0);
    let automix = needs
        .automix
        .then(|| {
            let end = notice.map_or(free_end, |n| n.x - gap);
            part(free_start, (end - free_start).max(0))
        })
        .filter(|r| r.w > 0);

    HeaderRects {
        bar: Rect::new(0, 0, window_w, header_height(m)),
        brand,
        automix,
        notice,
        latency,
        loudness,
        record,
        library_toggle,
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Density, ThemeTokens};

    const WIDTHS: [i32; 5] = [900, 1024, 1280, 1440, 1920];
    const FULL: HeaderNeeds = HeaderNeeds {
        automix: true,
        record: true,
        settings: true,
        notice: false,
    };

    fn metric_sets() -> [DensityMetrics; 2] {
        let t = ThemeTokens::builtin_dark();
        [*t.metrics(Density::Roomy), *t.metrics(Density::Compact)]
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn the_strip_is_one_control_high_with_the_comp_spacing() {
        let [roomy, compact] = metric_sets();
        assert_eq!(header_height(&roomy), 13 + 26 + 8 + 1);
        assert_eq!(header_height(&compact), 8 + 21 + 5 + 1);
    }

    #[test]
    fn every_part_is_inside_the_bar_on_one_baseline() {
        for m in metric_sets() {
            for w in WIDTHS {
                for needs in [
                    FULL,
                    HeaderNeeds {
                        notice: true,
                        ..FULL
                    },
                ] {
                    let rects = header_rects(w, &m, needs);
                    assert_eq!(rects.bar, Rect::new(0, 0, w, header_height(&m)));
                    for r in rects.all() {
                        assert!(rects.bar.contains(&r), "{w}: {r:?} outside the bar");
                        let top = m.space[3] as i32;
                        assert_eq!((r.y, r.h), (top, m.control_height as i32), "{w}: {r:?}");
                        assert!(r.w > 0, "{w}: {r:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn parts_do_not_overlap_and_run_in_order() {
        for m in metric_sets() {
            for w in WIDTHS {
                for needs in [
                    FULL,
                    HeaderNeeds {
                        notice: true,
                        ..FULL
                    },
                ] {
                    let rects = header_rects(w, &m, needs);
                    let all = rects.all();
                    for (i, a) in all.iter().enumerate() {
                        for b in &all[i + 1..] {
                            assert!(!overlap(*a, *b), "{w}: {a:?} over {b:?}");
                        }
                    }
                    let xs: Vec<i32> = all.iter().map(|r| r.x).collect();
                    assert!(xs.windows(2).all(|p| p[0] < p[1]), "{w}: {xs:?}");
                }
            }
        }
    }

    #[test]
    fn the_wordmark_has_its_measured_width() {
        for m in metric_sets() {
            assert!(header_rects(1280, &m, FULL).brand.w >= 124);
        }
    }

    #[test]
    fn automix_has_room_from_the_minimum_width() {
        for m in metric_sets() {
            for w in [1280, 1440, 1920] {
                let automix = header_rects(w, &m, FULL).automix.expect("automix");
                assert!(
                    automix.w >= AUTOMIX_FLOOR,
                    "{w}: automix is {} px",
                    automix.w
                );
            }
        }
    }

    #[test]
    fn a_notice_collapses_before_the_automix_bar_drops_under_its_floor() {
        for m in metric_sets() {
            for w in [1280, 1440, 1920] {
                let noisy = header_rects(
                    w,
                    &m,
                    HeaderNeeds {
                        notice: true,
                        ..FULL
                    },
                );
                let bar = noisy.automix.expect("bar");
                assert!(bar.w >= AUTOMIX_FLOOR, "{w}: bar {} px", bar.w);
                if w >= 1920 {
                    assert_eq!(noisy.notice.expect("chip").w, NOTICE_WIDTH);
                }
            }
        }
    }

    #[test]
    fn missing_parts_are_left_out() {
        let [m, _] = metric_sets();
        let margin = m.space[4] as i32;
        let bare = header_rects(1280, &m, HeaderNeeds::default());
        assert!(bare.automix.is_none() && bare.record.is_none() && bare.settings.is_none());
        assert_eq!(bare.library_toggle.right(), 1280 - margin);
        let settings = header_rects(
            1280,
            &m,
            HeaderNeeds {
                settings: true,
                ..HeaderNeeds::default()
            },
        );
        assert_eq!(settings.settings.expect("settings").right(), 1280 - margin);
    }
}
