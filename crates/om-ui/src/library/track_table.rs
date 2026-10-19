//! Table model for the library browser: paged (rows are fetched [`PAGE_SIZE`] at a time from a
//! [`TrackSource`] and a few pages are cached), with sortable columns, cell formatting and the key
//! compatibility rule against the loaded deck's key. The
//! paint code belongs to the gpui front end, which reads these values.

use std::collections::BTreeMap;

use om_analysis::{is_harmonically_compatible, MusicalKey};
use om_library::TrackRecord;

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
    pub width: i32,
    pub numeric: bool,
}

const COLUMNS: [Column; 8] = [
    Column {
        id: 1,
        title: "Title",
        sort_field: Some("title"),
        width: 260,
        numeric: false,
    },
    Column {
        id: 2,
        title: "Artist",
        sort_field: Some("artist"),
        width: 180,
        numeric: false,
    },
    Column {
        id: 3,
        title: "BPM",
        sort_field: Some("bpm"),
        width: 60,
        numeric: true,
    },
    Column {
        id: 4,
        title: "Key",
        sort_field: Some("key"),
        width: 52,
        numeric: true,
    },
    Column {
        id: 5,
        title: "Time",
        sort_field: Some("duration"),
        width: 56,
        numeric: true,
    },
    Column {
        id: 6,
        title: "Rating",
        sort_field: Some("rating"),
        width: 70,
        numeric: false,
    },
    Column {
        id: 7,
        title: "Genre",
        sort_field: Some("genre"),
        width: 110,
        numeric: false,
    },
    Column {
        id: 8,
        title: "Added",
        sort_field: Some("added"),
        width: 92,
        numeric: false,
    },
];

/// The browser's columns, in display order.
pub fn columns() -> &'static [Column] {
    &COLUMNS
}

#[derive(Default)]
pub struct TrackTableModel {
    source: Option<Box<dyn TrackSource>>,
    pages: BTreeMap<usize, Vec<TrackRecord>>,
    total: usize,
    reference: Option<MusicalKey>,
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
            6 => track.rating.to_string(),
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
