//! Where the header strip's parts sit: brand, automix bar, master BPM, notice chip, latency
//! indicator, Record button, library toggle and Settings button. Pure geometry shared by the window
//! and the page layout. Every part has the same top (`y == 3`) and height (`header_h - 6`), so the
//! controls share one baseline; the parts never overlap and run left to right in a fixed order.

use crate::geometry::Rect;

/// Space between the window edge and the first and last part.
pub const MARGIN: i32 = 8;
/// Space between neighbouring parts.
pub const GAP: i32 = 8;
/// Distance of every part from the top of the strip; the strip's bottom edge is as far again.
pub const TOP_INSET: i32 = 3;
/// Width of the OPENMIXER wordmark.
pub const BRAND_WIDTH: i32 = 112;
/// Width of the master BPM readout.
pub const MASTER_BPM_WIDTH: i32 = 120;
/// Width of the notice chip (two lines of caption text).
pub const NOTICE_WIDTH: i32 = 240;
/// Width of the notice chip collapsed to its icon and dismiss button.
pub const COLLAPSED_NOTICE_WIDTH: i32 = 56;
/// The automix bar's width the plan wants at 1280 wide; a notice chip collapses before the bar
/// goes under it.
pub const AUTOMIX_FLOOR: i32 = 300;
/// Width of the latency and dropout indicator.
pub const LATENCY_WIDTH: i32 = 120;
/// Width of the Record button.
pub const RECORD_WIDTH: i32 = 148;
/// Width of the library toggle (was PERFORM).
pub const LIBRARY_TOGGLE_WIDTH: i32 = 100;
/// Width of the Settings button.
pub const SETTINGS_WIDTH: i32 = 96;
/// The automix bar needs at least this much to show its segmented switch and the next track; a
/// window too narrow for it loses the master BPM readout first.
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
    /// The whole strip: `x = 0`, `y = 0`, the window's width and the header height.
    pub bar: Rect,
    /// The OPENMIXER wordmark, in the brand colour.
    pub brand: Rect,
    /// Segmented Off|On, the transition length and the next track.
    pub automix: Option<Rect>,
    /// The master tempo, centred on the window when it fits.
    pub master_bpm: Option<Rect>,
    /// A dismissible chip that replaces the old banner row.
    pub notice: Option<Rect>,
    /// Output latency and dropouts.
    pub latency: Rect,
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
            self.master_bpm,
            Some(self.latency),
            self.record,
            Some(self.library_toggle),
            self.settings,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Lays the strip out for a window `window_w` wide and a strip `header_h` high.
pub fn header_rects(window_w: i32, header_h: i32, needs: HeaderNeeds) -> HeaderRects {
    let h = (header_h - 2 * TOP_INSET).max(0);
    let part = |x: i32, w: i32| Rect::new(x, TOP_INSET, w, h);

    // The right cluster is anchored to the window's right edge.
    let mut edge = window_w - MARGIN;
    let mut take = |w: i32, wanted: bool| -> Option<Rect> {
        if !wanted {
            return None;
        }
        let rect = part(edge - w, w);
        edge = rect.x - GAP;
        Some(rect)
    };
    let settings = take(SETTINGS_WIDTH, needs.settings);
    let library_toggle = take(LIBRARY_TOGGLE_WIDTH, true).expect("always taken");
    let record = take(RECORD_WIDTH, needs.record);
    let latency = take(LATENCY_WIDTH, true).expect("always taken");

    let brand = part(MARGIN, BRAND_WIDTH);
    let free_start = brand.right() + GAP;
    let free_end = latency.x - GAP;

    // The master BPM sits on the window's centre line when it fits, else against the right
    // cluster; with an automix bar it also has to leave that bar its minimum.
    let centred = (window_w - MASTER_BPM_WIDTH) / 2;
    let bpm_x = if centred + MASTER_BPM_WIDTH <= free_end {
        centred
    } else {
        free_end - MASTER_BPM_WIDTH
    };
    let bpm_floor = if needs.automix {
        free_start + MIN_AUTOMIX_WIDTH + GAP
    } else {
        free_start
    };
    let master_bpm = (bpm_x >= bpm_floor).then(|| part(bpm_x, MASTER_BPM_WIDTH));

    let notice_end = master_bpm.map_or(free_end, |b| b.x - GAP);
    // The chip keeps the automix bar at or above 300 px (plan, header invariants): it takes its
    // full width only when that leaves the bar enough, and collapses to icon plus dismiss if not.
    let notice = needs.notice.then(|| {
        let room = notice_end - free_start - GAP - AUTOMIX_FLOOR;
        let w = if !needs.automix || room >= NOTICE_WIDTH {
            NOTICE_WIDTH.min((notice_end - free_start).max(0))
        } else {
            COLLAPSED_NOTICE_WIDTH
        };
        part(notice_end - w, w)
    });
    let automix = needs.automix.then(|| {
        let end = notice.map_or(notice_end, |n| n.x - GAP);
        part(free_start, (end - free_start).max(0))
    });

    HeaderRects {
        bar: Rect::new(0, 0, window_w, header_h),
        brand,
        automix,
        master_bpm,
        notice,
        latency,
        record,
        library_toggle,
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [(i32, i32); 5] = [
        (1280, 720),
        (1280, 800),
        (1920, 1080),
        (2560, 1440),
        (1100, 700),
    ];
    const FULL: HeaderNeeds = HeaderNeeds {
        automix: true,
        record: true,
        settings: true,
        notice: false,
    };

    fn header_h(window_h: i32) -> i32 {
        crate::main_layout::header_height(window_h)
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn every_part_is_inside_the_bar_on_one_baseline() {
        for (w, h) in SIZES {
            let hh = header_h(h);
            for needs in [
                FULL,
                HeaderNeeds {
                    notice: true,
                    ..FULL
                },
            ] {
                let rects = header_rects(w, hh, needs);
                assert_eq!(rects.bar, Rect::new(0, 0, w, hh));
                for r in rects.all() {
                    assert!(rects.bar.contains(&r), "{w}x{h}: {r:?} outside the bar");
                    assert_eq!((r.y, r.h), (3, hh - 6), "{w}x{h}: {r:?}");
                    assert!(r.w > 0, "{w}x{h}: {r:?}");
                }
            }
        }
    }

    #[test]
    fn parts_do_not_overlap_and_run_in_order() {
        for (w, h) in SIZES {
            for needs in [
                FULL,
                HeaderNeeds {
                    notice: true,
                    ..FULL
                },
            ] {
                let rects = header_rects(w, header_h(h), needs);
                let all = rects.all();
                for (i, a) in all.iter().enumerate() {
                    for b in &all[i + 1..] {
                        assert!(!overlap(*a, *b), "{w}x{h}: {a:?} over {b:?}");
                    }
                }
                let order = [
                    Some(rects.brand),
                    rects.automix,
                    rects.master_bpm,
                    Some(rects.latency),
                    rects.record,
                    Some(rects.library_toggle),
                    rects.settings,
                ];
                let xs: Vec<i32> = order.into_iter().flatten().map(|r| r.x).collect();
                assert!(xs.windows(2).all(|p| p[0] < p[1]), "{w}x{h}: {xs:?}");
            }
        }
    }

    #[test]
    fn master_bpm_is_centred_when_it_fits() {
        for (w, h) in SIZES.into_iter().filter(|s| s.0 >= 1280) {
            let rects = header_rects(w, header_h(h), FULL);
            let bpm = rects.master_bpm.expect("fits at every test size");
            assert!(
                (bpm.x + bpm.w / 2 - w / 2).abs() <= 1,
                "{w}x{h}: {bpm:?} is off centre"
            );
        }
    }

    #[test]
    fn automix_has_room_at_the_minimum_width() {
        for (w, h) in SIZES {
            let automix = header_rects(w, header_h(h), FULL).automix.expect("automix");
            assert!(automix.w >= 300, "{w}x{h}: automix is {} px", automix.w);
        }
    }

    #[test]
    fn a_notice_collapses_before_the_automix_bar_drops_under_300() {
        for (w, h) in [
            (1280, 720),
            (1280, 800),
            (1100, 700),
            (1920, 1080),
            (2560, 1440),
        ] {
            let plain = header_rects(w, header_h(h), FULL);
            let noisy = header_rects(
                w,
                header_h(h),
                HeaderNeeds {
                    notice: true,
                    ..FULL
                },
            );
            assert_eq!(plain.master_bpm, noisy.master_bpm, "{w}x{h}: bpm stays");
            let notice = noisy.notice.expect("chip");
            let bar = noisy.automix.expect("bar");
            assert!(bar.w >= 300 || w < 1280, "{w}x{h}: bar {} px", bar.w);
            if w == 1280 {
                assert_eq!(notice.w, COLLAPSED_NOTICE_WIDTH);
            }
            if w >= 1920 {
                assert_eq!(notice.w, NOTICE_WIDTH, "{w}: wide windows show the text");
            }
            assert!(plain.notice.is_none());
        }
    }

    #[test]
    fn missing_parts_are_left_out() {
        let bare = header_rects(1280, 36, HeaderNeeds::default());
        assert!(bare.automix.is_none() && bare.record.is_none() && bare.settings.is_none());
        let settings_only = header_rects(
            1280,
            36,
            HeaderNeeds {
                settings: true,
                ..bare_needs()
            },
        );
        assert_eq!(
            settings_only.settings.expect("settings").right(),
            1280 - MARGIN
        );
        assert_eq!(bare.library_toggle.right(), 1280 - MARGIN);
    }

    fn bare_needs() -> HeaderNeeds {
        HeaderNeeds::default()
    }

    #[test]
    fn a_narrow_window_loses_the_master_bpm_before_the_automix_bar() {
        let rects = header_rects(900, 36, FULL);
        assert!(rects.master_bpm.is_none());
        assert!(rects.automix.expect("automix").w >= MIN_AUTOMIX_WIDTH);
    }
}
