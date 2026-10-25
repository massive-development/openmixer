//! Table model for the library browser: paged (rows are fetched [`PAGE_SIZE`] at a time from a
//! [`TrackSource`] and a few pages are cached), with sortable columns, cell formatting and the key
//! compatibility rule against the loaded deck's key. The
//! paint code belongs to the gpui front end, which reads these values.

use std::collections::BTreeMap;

use om_analysis::{is_harmonically_compatible, MusicalKey};
use om_library::TrackRecord;

use super::mix_match::{self, MixLevel, MixMatch, MixReference};
use super::track_drag;
use super::track_source::TrackSource;
use crate::format;

/// Rows per fetched page.
pub const PAGE_SIZE: usize = 100;
/// Pages kept in memory at once.
pub const MAX_CACHED_PAGES: usize = 6;

/// One column of the browser table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub id: i32,
    pub title: &'static str,
    /// Search field the column sorts by; `None` = not sortable.
    pub sort_field: Option<&'static str>,
    /// Width in px; 0 = flexible (Title and Artist share what the fixed columns leave).
    pub width: i32,
    /// Right-aligned.
    pub numeric: bool,
    /// Hidden in windows narrower than this (0 = always shown).
    pub min_window_w: i32,
}

const COLUMNS: [Column; 9] = [
    Column {
        id: 1,
        title: "Title",
        sort_field: Some("title"),
        width: 0,
        numeric: false,
        min_window_w: 0,
    },
    Column {
        id: 2,
        title: "Artist",
        sort_field: Some("artist"),
        width: 0,
        numeric: false,
        min_window_w: 0,
    },
    Column {
        id: 3,
        title: "BPM",
        sort_field: Some("bpm"),
        width: 64,
        numeric: true,
        min_window_w: 0,
    },
    Column {
        id: 4,
        title: "Key",
        sort_field: Some("key"),
        width: 56,
        numeric: true,
        min_window_w: 0,
    },
    Column {
        id: 5,
        title: crate::labels::LENGTH_COLUMN,
        sort_field: Some("duration"),
        width: 64,
        numeric: true,
        min_window_w: 0,
    },
    Column {
        id: 6,
        title: "Rating",
        sort_field: Some("rating"),
        width: 80,
        numeric: false,
        min_window_w: 0,
    },
    Column {
        id: 7,
        title: "Genre",
        sort_field: Some("genre"),
        width: 120,
        numeric: false,
        min_window_w: 1400,
    },
    Column {
        id: 8,
        title: "Added",
        sort_field: Some("added"),
        width: 96,
        numeric: false,
        min_window_w: 1400,
    },
    Column {
        id: STEMS_COLUMN,
        title: "Stems",
        sort_field: None,
        width: 96,
        numeric: false,
        min_window_w: 0,
    },
];

/// The stems indicator column. Its text comes from `StemQueue::label` (the table has no access to
/// the stem jobs), so `cell_text` leaves it empty.
pub const STEMS_COLUMN: i32 = 9;

/// The browser's columns, in display order.
pub fn columns() -> &'static [Column] {
    &COLUMNS
}

/// Whether `column` is shown in a window `window_w` px wide.
pub fn column_shown(column: &Column, window_w: i32) -> bool {
    window_w >= column.min_window_w
}

/// A rating as five characters, `*` for a star and `-` for an empty slot ("***--" for 3). The view
/// maps them to glyphs.
pub fn rating_stars(rating: i32) -> String {
    let stars = rating.clamp(0, 5) as usize;
    format!("{}{}", "*".repeat(stars), "-".repeat(5 - stars))
}

/// Which way a column header's sort glyph points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortGlyph {
    None,
    Up,
    Down,
}

/// The glyph for column `column_id` given the table's current sort `(column, ascending)`.
pub fn sort_glyph(sort: Option<(i32, bool)>, column_id: i32) -> SortGlyph {
    match sort {
        Some((column, true)) if column == column_id => SortGlyph::Up,
        Some((column, false)) if column == column_id => SortGlyph::Down,
        _ => SortGlyph::None,
    }
}

#[derive(Default)]
pub struct TrackTableModel {
    source: Option<Box<dyn TrackSource>>,
    pages: BTreeMap<usize, Vec<TrackRecord>>,
    total: usize,
    reference: Option<MusicalKey>,
    reference_bpm: Option<f64>,
    /// The error from the last page that could not be read; empty when the last fetch worked.
    fetch_error: String,
}

impl TrackTableModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the source, drops cached pages and counts the new rows.
    pub fn set_source(&mut self, source: Box<dyn TrackSource>) {
        self.source = Some(source);
        self.reload();
    }

    /// Same source, fresh data.
    pub fn reload(&mut self) {
        self.pages.clear();
        self.fetch_error.clear();
        self.total = self.source.as_mut().map_or(0, |s| s.count());
    }

    /// Why the source failed or rejected its query, or why the last page could not be read; empty
    /// when fine.
    pub fn source_error(&self) -> String {
        if !self.fetch_error.is_empty() {
            return self.fetch_error.clone();
        }
        self.source.as_ref().map_or_else(String::new, |s| s.error())
    }

    pub fn num_rows(&self) -> usize {
        self.total
    }

    /// Camelot code to mix with (`""` = none); rows with a compatible key are tinted.
    pub fn set_reference_camelot(&mut self, camelot: &str) {
        self.reference = if camelot.is_empty() {
            None
        } else {
            MusicalKey::from_camelot(camelot)
        };
    }

    /// Tempo of the reference track (`None` = unknown); with a reference key it sharpens the tint.
    pub fn set_reference_bpm(&mut self, bpm: Option<f64>) {
        self.reference_bpm = bpm;
    }

    /// Key relation and tempo distance of `track` against the reference.
    pub fn mix_match(&self, track: &TrackRecord) -> MixMatch {
        let reference = MixReference {
            camelot: self
                .reference
                .as_ref()
                .map(MusicalKey::camelot)
                .unwrap_or_default(),
            bpm: self.reference_bpm,
        };
        mix_match::score(&reference, &track.camelot, track.bpm)
    }

    /// How strongly the row stands out against the reference.
    pub fn mix_level(&self, track: &TrackRecord) -> MixLevel {
        self.mix_match(track).level
    }

    pub fn reference_camelot_key(&self) -> Option<&MusicalKey> {
        self.reference.as_ref()
    }

    pub fn is_compatible(&self, track: &TrackRecord) -> bool {
        let Some(reference) = &self.reference else {
            return false;
        };
        if track.camelot.is_empty() {
            return false;
        }
        MusicalKey::from_camelot(&track.camelot)
            .is_some_and(|key| is_harmonically_compatible(reference, &key))
    }

    /// The row at `row`, fetching its page when needed. `None` outside the list.
    pub fn row_at(&mut self, row: isize) -> Option<&TrackRecord> {
        let row = usize::try_from(row).ok().filter(|&r| r < self.total)?;
        let page = row / PAGE_SIZE;
        if !self.pages.contains_key(&page) {
            if self.pages.len() >= MAX_CACHED_PAGES {
                self.evict_farthest_from(page);
            }
            let fetched = self
                .source
                .as_mut()
                .map_or_else(|| Ok(Vec::new()), |s| s.fetch(page * PAGE_SIZE, PAGE_SIZE));
            match fetched {
                Ok(rows) => {
                    self.fetch_error.clear();
                    self.pages.insert(page, rows);
                }
                // Not cached: the next read of this page asks the source again.
                Err(error) => {
                    self.fetch_error = error;
                    return None;
                }
            }
        }
        self.pages.get(&page)?.get(row % PAGE_SIZE)
    }

    /// Scrolling is local, so the page farthest from the one being read is the least useful.
    fn evict_farthest_from(&mut self, page: usize) {
        let mut farthest: Option<usize> = None;
        for &candidate in self.pages.keys() {
            let distance = candidate.abs_diff(page);
            match farthest {
                Some(f) if f.abs_diff(page) >= distance => {}
                _ => farthest = Some(candidate),
            }
        }
        if let Some(f) = farthest {
            self.pages.remove(&f);
        }
    }

    /// Text for a cell; empty for rows or columns that do not exist.
    pub fn cell_text(&mut self, row: isize, column_id: i32) -> String {
        let Some(track) = self.row_at(row).cloned() else {
            return String::new();
        };
        match column_id {
            1 => {
                if track.title.is_empty() {
                    file_stem(&track.path)
                } else {
                    track.title
                }
            }
            2 => track.artist,
            3 => format_bpm(track.bpm),
            4 => {
                if track.camelot.is_empty() {
                    track.key
                } else {
                    track.camelot
                }
            }
            5 => format_duration(track.duration_sec),
            6 => rating_stars(track.rating),
            7 => track.genre,
            8 => format_added(track.date_added_ms),
            _ => String::new(),
        }
    }

    /// Header click: the sort field and direction for the next source. `forwards` is the header's
    /// own direction, so a forward click sorts descending. `None` for
    /// columns that cannot sort.
    pub fn sort_order_changed(column_id: i32, forwards: bool) -> Option<(&'static str, bool)> {
        columns()
            .iter()
            .find(|c| c.id == column_id)
            .and_then(|c| c.sort_field)
            .map(|field| (field, !forwards))
    }

    /// The track to load when the row is activated (double-click or Enter).
    pub fn activate(&mut self, row: isize) -> Option<TrackRecord> {
        self.row_at(row).cloned()
    }

    /// Drag payload for the selected rows: the first one. `None` when
    /// nothing is selected.
    pub fn drag_source_description(&mut self, rows: &[usize]) -> Option<String> {
        let first = *rows.first()?;
        let track = self.row_at(first as isize)?;
        Some(track_drag::describe(track.id))
    }
}

/// `4:05` for a length in seconds.
pub fn format_duration(seconds: f64) -> String {
    format::clock(seconds, false)
}

/// `128.0`, or `--` when the track has no BPM yet.
pub fn format_bpm(bpm: Option<f64>) -> String {
    match bpm {
        Some(value) => format!("{value:.1}"),
        None => "--".to_owned(),
    }
}

/// `yyyy-mm-dd` (UTC) for a timestamp in milliseconds; empty when unknown.
pub fn format_added(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let days = ms.div_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The file name without its extension, as the title fallback shows it.
fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_follow_the_plan_and_genre_and_added_hide_in_narrow_windows() {
        let by_title = |t: &str| *columns().iter().find(|c| c.title == t).expect(t);
        assert_eq!(by_title("Length").title, "Length");
        assert_eq!(
            ["BPM", "Key", "Length", "Rating", "Genre", "Added"].map(|t| by_title(t).width),
            [64, 56, 64, 80, 120, 96]
        );
        assert_eq!((by_title("Title").width, by_title("Artist").width), (0, 0));
        assert!(by_title("BPM").numeric && by_title("Length").numeric);
        for narrow in [1280, 1399] {
            assert!(!column_shown(&by_title("Genre"), narrow));
            assert!(!column_shown(&by_title("Added"), narrow));
            assert!(column_shown(&by_title("Rating"), narrow));
        }
        assert!(column_shown(&by_title("Genre"), 1400));
    }

    #[test]
    fn ratings_are_five_star_strings_and_sort_has_a_glyph_model() {
        assert_eq!(rating_stars(3), "***--");
        assert_eq!(rating_stars(0), "-----");
        assert_eq!(rating_stars(9), "*****");
        assert_eq!(sort_glyph(None, 3), SortGlyph::None);
        assert_eq!(sort_glyph(Some((3, true)), 3), SortGlyph::Up);
        assert_eq!(sort_glyph(Some((3, false)), 3), SortGlyph::Down);
        assert_eq!(sort_glyph(Some((3, true)), 4), SortGlyph::None);
    }

    #[test]
    fn formats_match_the_cpp_table() {
        assert_eq!(format_bpm(Some(120.0)), "120.0");
        assert_eq!(format_bpm(None), "--");
        assert_eq!(format_duration(245.0), "4:05");
        assert_eq!(format_added(0), "");
        assert_eq!(format_added(1_700_000_000_000).len(), 10);
        assert_eq!(format_added(1_700_000_000_000), "2023-11-14");
    }

    #[test]
    fn header_clicks_sort_descending_on_a_forward_click() {
        assert_eq!(
            TrackTableModel::sort_order_changed(3, false),
            Some(("bpm", true))
        );
        assert_eq!(
            TrackTableModel::sort_order_changed(2, true),
            Some(("artist", false))
        );
        assert_eq!(TrackTableModel::sort_order_changed(99, true), None);
    }

    #[test]
    fn every_sortable_column_names_a_field() {
        let sortable = columns().iter().filter(|c| c.sort_field.is_some()).count();
        assert_eq!(sortable, 8, "every column sorts");
    }

    /// Fails its first fetch, then serves three rows.
    struct FlakySource {
        failures_left: usize,
    }

    impl TrackSource for FlakySource {
        fn count(&mut self) -> usize {
            3
        }

        fn fetch(&mut self, offset: usize, limit: usize) -> Result<Vec<TrackRecord>, String> {
            if self.failures_left > 0 {
                self.failures_left -= 1;
                return Err("database is locked".to_owned());
            }
            Ok((offset..3.min(offset + limit))
                .map(|i| TrackRecord {
                    id: i as i64 + 1,
                    title: format!("T{i}"),
                    ..TrackRecord::default()
                })
                .collect())
        }
    }

    #[test]
    fn a_failed_page_is_not_cached_and_its_error_is_reported() {
        let mut model = TrackTableModel::new();
        model.set_source(Box::new(FlakySource { failures_left: 1 }));
        assert!(model.row_at(0).is_none(), "no rows from a failed page");
        assert!(model.source_error().contains("database is locked"));

        let row = model
            .row_at(0)
            .expect("the next read fetches the page again");
        assert_eq!(row.title, "T0");
        assert!(
            model.source_error().is_empty(),
            "a good page clears the error"
        );
    }
}
