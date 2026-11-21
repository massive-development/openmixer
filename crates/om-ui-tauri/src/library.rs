//! The library browser as the front end reads it. The hub (services, loader, browser) lives on the
//! control thread and is not `Send`, so a command hands the control thread a closure over the
//! [`LibraryPanel`] and waits for its answer: the closure runs within one control tick (30 Hz), so
//! a call takes up to about 33 ms. Nothing here touches the audio thread.
//!
//! [`LibraryView`] is everything the browser draws in one answer: the sidebar, the toolbar's state,
//! the status line, the analysis display and the rows that are on screen. The behaviour (search,
//! sort, selection, playlists, loads) is the panel's own; this file only names it for the wire.

use crate::lock;
use om_library::{PrepEntry, PrepStatus, TrackRecord};
use om_ui::library::hub::{LibraryPanel, RowView};
use om_ui::library::{columns, Column, MixLevel, SEARCH_PLACEHOLDER, STEMS_COLUMN};
use om_ui::sidebar::SidebarKind;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;

/// How long a command waits for the control thread before it reports the library as busy.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// The most rows one view carries; a screenful with its overscan is far below this.
pub const MAX_VIEW_ROWS: usize = 200;

type Job = Box<dyn FnOnce(&mut LibraryPanel) + Send>;

/// Work for the control thread's panel, queued by the commands.
#[derive(Default)]
pub(crate) struct LibraryQueue {
    jobs: Mutex<Vec<Job>>,
}

impl LibraryQueue {
    /// Runs `work` on the control thread and returns its answer. `Err` says why there is none: no
    /// library is open, or the control thread did not get to it in time.
    pub(crate) fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut LibraryPanel) -> T + Send + 'static,
    ) -> Result<T, String> {
        let (reply, answer) = mpsc::channel();
        lock(&self.jobs).push(Box::new(move |panel| {
            // The caller may have given up; nothing else wants the answer.
            let _ = reply.send(work(panel));
        }));
        answer.recv_timeout(CALL_TIMEOUT).map_err(|error| match error {
            RecvTimeoutError::Timeout => "the library is busy".to_owned(),
            RecvTimeoutError::Disconnected => "no library is open".to_owned(),
        })
    }

    /// Runs what the commands queued (the control thread, before its tick).
    pub(crate) fn run(&self, panel: &mut LibraryPanel) {
        let jobs = std::mem::take(&mut *lock(&self.jobs));
        for job in jobs {
            job(panel);
        }
    }

    /// Drops what is queued, so each waiting command fails at once (no library, or shutting down).
    pub(crate) fn discard(&self) {
        lock(&self.jobs).clear();
    }
}

/// One sidebar line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarDto {
    /// Position in the flattened tree; `library_select_sidebar` takes it back.
    pub index: usize,
    pub depth: usize,
    /// `all`, `playlists`, `folder`, `playlist`, `watches`, `watch` or `prep`.
    pub kind: &'static str,
    pub label: String,
    pub playlist_id: i64,
    pub selected: bool,
    pub selectable: bool,
}

/// One table column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDto {
    pub id: i32,
    pub title: &'static str,
    pub sortable: bool,
    /// Width in px; 0 shares what the fixed columns leave.
    pub width: i32,
    pub numeric: bool,
    /// Hidden in windows narrower than this; 0 always shows.
    pub min_window_w: i32,
}

/// One visible row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDto {
    pub row: usize,
    pub track_id: i64,
    /// One string per column of the view, in order.
    pub cells: Vec<String>,
    /// Key fits the reference deck (the tint).
    pub compatible: bool,
    /// `strong` (key and tempo fit), `key` or `none`.
    pub mix: &'static str,
    pub selected: bool,
    /// The deck the track is loaded on.
    pub on_deck: Option<usize>,
    /// Why the track would not load, in words.
    pub error: Option<&'static str>,
    /// Analysis progress `0..=1` while the track is queued or running; absent otherwise.
    pub preparing: Option<f32>,
}

/// A playlist name being typed in the search box's place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NamingDto {
    pub prompt: &'static str,
    pub text: String,
}

/// The analysis display: the prep queue's state and what is known of the first selected track.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDto {
    pub queued: usize,
    pub running: usize,
    pub failed: usize,
    /// Mean progress of the running tracks, `0..=1`.
    pub progress: f32,
    pub track: Option<TrackAnalysisDto>,
}

/// What the library knows of one track's analysis.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackAnalysisDto {
    pub title: String,
    pub artist: String,
    pub bpm: Option<f64>,
    pub key: String,
    pub camelot: String,
    pub duration_sec: f64,
    /// Tempo and key are both known.
    pub analysed: bool,
    /// `queued`, `running`, `failed`, or absent.
    pub prep: Option<&'static str>,
}

/// Everything the browser shows, in one answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub sidebar: Vec<SidebarDto>,
    pub columns: Vec<ColumnDto>,
    pub search: String,
    pub placeholder: &'static str,
    pub naming: Option<NamingDto>,
    pub status: String,
    pub status_is_error: bool,
    pub row_count: usize,
    pub selected: Vec<usize>,
    /// Column id and direction (`true` ascending) of the last header click.
    pub sort: Option<(i32, bool)>,
    /// Stems are available (a host is attached): the table shows its column and the toolbar its buttons.
    pub stems_available: bool,
    pub stems_in_flight: usize,
    /// The first of `rows`.
    pub first: usize,
    pub rows: Vec<RowDto>,
    pub analysis: AnalysisDto,
}

/// Reads the panel: `count` rows from `first` (at most [`MAX_VIEW_ROWS`]).
pub(crate) fn view(panel: &mut LibraryPanel, first: usize, count: usize) -> LibraryView {
    let stems_available = panel.hub().borrow().stem_host.is_some();
    let shown = shown_columns(stems_available);
    let row_count = panel.row_count();
    let first = first.min(row_count);
    let last = first.saturating_add(count.min(MAX_VIEW_ROWS)).min(row_count);
    let prep = prep_entries(panel);
    let rows = (first..last)
        .filter_map(|row| panel.row(row))
        .map(|row| row_dto(row, &shown, &prep))
        .collect();
    LibraryView {
        sidebar: sidebar(panel),
        columns: shown.iter().map(|c| column_dto(c)).collect(),
        search: panel.browser.search_text().to_owned(),
        placeholder: SEARCH_PLACEHOLDER,
        naming: panel.name_entry.as_ref().map(|n| NamingDto {
            prompt: n.prompt(),
            text: n.text.clone(),
        }),
        status: om_ui::labels::library_status(panel.browser.status(), row_count),
        status_is_error: panel.browser.status_is_error(),
        row_count,
        selected: panel.browser.selected_rows(),
        sort: panel.sort_indicator(),
        stems_available,
        stems_in_flight: panel.stems_in_flight(),
        first,
        rows,
        analysis: analysis(panel, &prep),
    }
}

/// The table's columns; the stems column only when there are stems to show.
fn shown_columns(stems_available: bool) -> Vec<&'static Column> {
    columns()
        .iter()
        .filter(|c| c.id != STEMS_COLUMN || stems_available)
        .collect()
}

fn column_dto(column: &Column) -> ColumnDto {
    ColumnDto {
        id: column.id,
        title: column.title,
        sortable: column.sort_field.is_some(),
        width: column.width,
        numeric: column.numeric,
        min_window_w: column.min_window_w,
    }
}

fn sidebar(panel: &LibraryPanel) -> Vec<SidebarDto> {
    panel
        .sidebar_lines()
        .into_iter()
        .enumerate()
        .map(|(index, line)| SidebarDto {
            index,
            depth: line.depth,
            kind: kind_name(line.node.kind),
            selectable: line.node.is_selectable(),
            label: line.node.label,
            playlist_id: line.node.playlist_id,
            selected: line.selected,
        })
        .collect()
}

fn kind_name(kind: SidebarKind) -> &'static str {
    match kind {
        SidebarKind::AllTracks => "all",
        SidebarKind::PlaylistGroup => "playlists",
        SidebarKind::Folder => "folder",
        SidebarKind::Playlist => "playlist",
        SidebarKind::WatchGroup => "watches",
        SidebarKind::WatchFolder => "watch",
        SidebarKind::PrepQueue => "prep",
    }
}

fn row_dto(row: RowView, shown: &[&Column], prep: &HashMap<i64, PrepEntry>) -> RowDto {
    let cells = columns()
        .iter()
        .zip(row.cells)
        .filter(|(column, _)| shown.iter().any(|s| s.id == column.id))
        .map(|(_, cell)| cell)
        .collect();
    RowDto {
        row: row.row,
        track_id: row.track_id,
        cells,
        compatible: row.compatible,
        mix: mix_name(row.mix),
        selected: row.selected,
        on_deck: row.on_deck,
        error: row.error,
        preparing: prep.get(&row.track_id).and_then(preparing),
    }
}

fn mix_name(mix: MixLevel) -> &'static str {
    match mix {
        MixLevel::Strong => "strong",
        MixLevel::Key => "key",
        MixLevel::None => "none",
    }
}

/// Progress of an entry that is still being worked on.
fn preparing(entry: &PrepEntry) -> Option<f32> {
    match entry.status {
        PrepStatus::Queued => Some(0.0),
        PrepStatus::Running => Some(entry.progress.clamp(0.0, 1.0)),
        PrepStatus::Done | PrepStatus::Failed | PrepStatus::Cancelled => None,
    }
}

/// The prep queue by track; empty when it cannot be read (the status line reports library errors).
fn prep_entries(panel: &LibraryPanel) -> HashMap<i64, PrepEntry> {
    let hub = panel.hub().borrow();
    let entries = hub.services.prep_queue().list().unwrap_or_default();
    entries.into_iter().map(|e| (e.track_id, e)).collect()
}

fn analysis(panel: &mut LibraryPanel, prep: &HashMap<i64, PrepEntry>) -> AnalysisDto {
    let count = |status: PrepStatus| prep.values().filter(|e| e.status == status).count();
    let running: Vec<f32> = prep
        .values()
        .filter(|e| e.status == PrepStatus::Running)
        .map(|e| e.progress)
        .collect();
    let progress = if running.is_empty() {
        0.0
    } else {
        running.iter().sum::<f32>() / running.len() as f32
    };
    let track = panel
        .browser
        .selected_row()
        .and_then(|row| panel.browser.model.row_at(row as isize).cloned())
        .map(|track| track_analysis(&track, prep));
    AnalysisDto {
        queued: count(PrepStatus::Queued),
        running: running.len(),
        failed: count(PrepStatus::Failed),
        progress,
        track,
    }
}

fn track_analysis(track: &TrackRecord, prep: &HashMap<i64, PrepEntry>) -> TrackAnalysisDto {
    let state = prep.get(&track.id).map(|e| e.status);
    TrackAnalysisDto {
        title: track.title.clone(),
        artist: track.artist.clone(),
        bpm: track.bpm.filter(|bpm| *bpm > 0.0),
        key: track.key.clone(),
        camelot: track.camelot.clone(),
        duration_sec: track.duration_sec,
        analysed: track.bpm.is_some_and(|bpm| bpm > 0.0) && !track.camelot.is_empty(),
        prep: match state {
            Some(PrepStatus::Queued) => Some("queued"),
            Some(PrepStatus::Running) => Some("running"),
            Some(PrepStatus::Failed) => Some("failed"),
            _ => None,
        },
    }
}

/// A header, a name box or a toolbar button's action on the panel, as the front end names it.
pub(crate) fn name_action(panel: &mut LibraryPanel, action: &str, text: Option<&str>) -> bool {
    match action {
        "new-playlist" => panel.begin_new_playlist(),
        "new-folder" => panel.begin_new_folder(),
        "rename" => panel.begin_rename_playlist(),
        "text" => panel.set_name_text(text.unwrap_or_default()),
        "submit" => {
            panel.submit_name();
        }
        "cancel" => panel.cancel_name(),
        _ => return false,
    }
    true
}

/// A key the browser uses, by name; the answer is what it did (`search`, `moved`, `activated`,
/// `edited` or `ignored`) so the front end can focus the search box or scroll to the selection.
pub(crate) fn key(panel: &mut LibraryPanel, key: &str) -> &'static str {
    use om_ui::library::hub::KeyOutcome;
    match panel.handle_key(key) {
        KeyOutcome::Ignored => "ignored",
        KeyOutcome::Search => "search",
        KeyOutcome::Moved => "moved",
        KeyOutcome::Activated => "activated",
        KeyOutcome::Edited => "edited",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_without_a_control_thread_times_out_and_a_discarded_one_fails_at_once() {
        let queue = std::sync::Arc::new(LibraryQueue::default());
        let waiting = std::sync::Arc::clone(&queue);
        let call = std::thread::spawn(move || waiting.call(|_| 1));
        while lock(&queue.jobs).is_empty() {
            std::thread::yield_now();
        }
        queue.discard();
        assert_eq!(call.join().unwrap(), Err("no library is open".to_owned()));
    }

    #[test]
    fn the_stems_column_shows_only_with_stems() {
        assert!(shown_columns(true).iter().any(|c| c.id == STEMS_COLUMN));
        assert!(!shown_columns(false).iter().any(|c| c.id == STEMS_COLUMN));
    }

    #[test]
    fn a_queued_or_running_entry_is_preparing_and_a_finished_one_is_not() {
        let entry = |status, progress| PrepEntry {
            track_id: 1,
            position: 0,
            priority: 0,
            status,
            progress,
            error: String::new(),
        };
        assert_eq!(preparing(&entry(PrepStatus::Queued, 0.0)), Some(0.0));
        assert_eq!(preparing(&entry(PrepStatus::Running, 1.5)), Some(1.0));
        assert_eq!(preparing(&entry(PrepStatus::Done, 1.0)), None);
        assert_eq!(preparing(&entry(PrepStatus::Failed, 0.2)), None);
    }
}
