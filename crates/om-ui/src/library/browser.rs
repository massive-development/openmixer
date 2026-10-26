//! The library browser's logic: the track table, the sidebar selection, the search box (applied on
//! the next tick, not per keystroke), the status line, load requests to the decks, prep queueing and
//! XML import. The browser does not own the services: each action takes them, so the owner keeps the
//! one mutable handle. Logic only; the view is in gpui.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};

use om_library::{ImportReport, LibraryError, PrepStatus, TrackRecord};
use std::path::PathBuf;

use super::control::import_summary;
use super::services::{LibraryEvent, LibraryServices};
use super::track_loader::LoadError;
use super::track_source::{PlaylistSource, PrepQueueSource, SearchTrackSource, TrackSource};
use super::track_table::TrackTableModel;
use crate::settings::SettingsResult;
use crate::sidebar::{build_sidebar, SidebarKind, SidebarNode};
use crate::stems::{StemHost, StemJob, StemQueue};

/// Placeholder shown in the empty search box.
pub const SEARCH_PLACEHOLDER: &str =
    "Search: artist:name bpm:120-128 key:~8A -genre:techno sort:-added";

/// Called with the deck (0-based) and the track when the user asks to load it. An error is shown in
/// the status line.
pub type LoadHandler = Box<dyn FnMut(usize, &TrackRecord) -> Result<(), LoadError>>;

/// The browser's current search text, published for other readers (the automix queue's "current
/// search" source). Written on every change, so it is the text the user sees, not the last query run.
pub type SearchFeed = std::sync::Arc<std::sync::Mutex<String>>;

pub struct LibraryBrowser {
    pub model: TrackTableModel,
    /// Receives the search text as it changes. `None` publishes nothing.
    pub search_feed: Option<SearchFeed>,
    /// Where a double-click loads. Returns the deck index; defaults to deck 0.
    pub default_deck: Option<Box<dyn Fn() -> usize>>,
    /// Camelot code of the reference key, polled on each tick.
    pub reference_key: Option<Box<dyn Fn() -> String>>,
    pub on_load_requested: Option<LoadHandler>,
    num_decks: usize,
    nodes: Vec<SidebarNode>,
    selected: SidebarNode,
    search_text: String,
    search_pending: bool,
    stale: bool,
    rebuild_sidebar: bool,
    sort_field: String,
    sort_descending: bool,
    status: String,
    status_is_error: bool,
    /// Why part of the sidebar could not be read; shown until the library reads cleanly again.
    sidebar_error: Option<String>,
    shown_reference: String,
    /// The page error last shown in the status line; empty when none.
    shown_page_error: String,
    selected_rows: BTreeSet<usize>,
    events: Receiver<LibraryEvent>,
    /// Tracks whose analysis finished since the last [`LibraryBrowser::take_analysed`].
    analysed: Vec<i64>,
    /// The playlist a first Delete press named; a second press on the same one removes it.
    delete_armed: Option<i64>,
}

impl LibraryBrowser {
    /// Builds the sidebar from the library and shows all tracks.
    pub fn new(services: &mut LibraryServices, num_decks: usize) -> Self {
        let events = services.subscribe();
        let sidebar = build_sidebar(services.library());
        let sidebar_error = sidebar.error.as_ref().map(ToString::to_string);
        let nodes = sidebar.nodes;
        let selected = nodes.first().cloned().unwrap_or_else(|| SidebarNode {
            kind: SidebarKind::AllTracks,
            label: "All tracks".to_owned(),
            playlist_id: 0,
            path: String::new(),
            children: Vec::new(),
        });
        let mut browser = Self {
            model: TrackTableModel::new(),
            search_feed: None,
            default_deck: None,
            reference_key: None,
            on_load_requested: None,
            num_decks,
            nodes,
            selected,
            search_text: String::new(),
            search_pending: false,
            stale: false,
            rebuild_sidebar: false,
            sort_field: String::new(),
            sort_descending: false,
            status: String::new(),
            status_is_error: false,
            sidebar_error,
            shown_reference: String::new(),
            shown_page_error: String::new(),
            selected_rows: BTreeSet::new(),
            events,
            analysed: Vec::new(),
            delete_armed: None,
        };
        browser.apply_query(services);
        browser
    }

    pub fn num_decks(&self) -> usize {
        self.num_decks
    }

    pub fn sidebar_nodes(&self) -> &[SidebarNode] {
        &self.nodes
    }

    pub fn selected_node(&self) -> &SidebarNode {
        &self.selected
    }

    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    fn publish_search(&self) {
        if let Some(feed) = &self.search_feed {
            let mut text = feed.lock().unwrap_or_else(|e| e.into_inner());
            text.clear();
            text.push_str(&self.search_text);
        }
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn status_is_error(&self) -> bool {
        self.status_is_error
    }

    /// Replaces the search text and applies it now.
    pub fn set_search_text(&mut self, services: &LibraryServices, text: &str) {
        self.search_text = text.to_owned();
        self.publish_search();
        self.search_pending = false;
        self.apply_query(services);
    }

    /// The user typed. The query runs on the next [`Self::flush_pending`], not per keystroke.
    pub fn type_search(&mut self, text: &str) {
        self.search_text = text.to_owned();
        self.publish_search();
        self.search_pending = true;
    }

    /// Selects a sidebar node by kind and id (playlist id), or by path for a watch folder. Groups and
    /// unknown nodes are not selectable; returns whether the node was found.
    pub fn select_sidebar(
        &mut self,
        services: &LibraryServices,
        kind: SidebarKind,
        id: i64,
        path: &str,
    ) -> bool {
        let found = find_node(&self.nodes, &|n| {
            n.is_selectable()
                && n.kind == kind
                && (kind != SidebarKind::Playlist || n.playlist_id == id)
                && (kind != SidebarKind::WatchFolder || n.path == path)
        });
        match found {
            Some(node) => {
                self.selected = node;
                self.apply_query(services);
                true
            }
            None => false,
        }
    }

    /// A column header was clicked: sort through the search engine.
    pub fn header_sort(
        &mut self,
        services: &LibraryServices,
        column_id: i32,
        forwards: bool,
    ) -> bool {
        match TrackTableModel::sort_order_changed(column_id, forwards) {
            Some((field, descending)) => {
                self.sort_field = field.to_owned();
                self.sort_descending = descending;
                self.apply_query(services);
                true
            }
            None => false,
        }
    }

    /// Selects one row, replacing the selection.
    pub fn select_row(&mut self, row: usize) {
        self.selected_rows = BTreeSet::from([row]);
    }

    /// Selects the inclusive range `first..=last`, replacing the selection.
    pub fn select_range(&mut self, first: usize, last: usize) {
        self.selected_rows = (first.min(last)..=first.max(last)).collect();
    }

    pub fn selected_rows(&self) -> Vec<usize> {
        self.selected_rows.iter().copied().collect()
    }

    /// The first selected row, as the table reports it.
    pub fn selected_row(&self) -> Option<usize> {
        self.selected_rows.iter().next().copied()
    }

    /// Loads the first selected row into `deck`. With nothing selected, the status says so.
    pub fn click_load(&mut self, deck: usize) {
        if self.on_load_requested.is_none() {
            return;
        }
        let track = self
            .selected_row()
            .and_then(|row| self.model.row_at(row as isize).cloned());
        match track {
            Some(track) => {
                // A refused load (no handler, or the deck failed) is already shown in the status.
                let _ = self.request_load(deck, &track);
            }
            None => self.set_status("Select a track first".to_owned(), true),
        }
    }

    /// Activation (double-click or Enter) loads the row into the default deck.
    pub fn activate_row(&mut self, row: usize) -> Option<Result<(), LoadError>> {
        let track = self.model.activate(row as isize)?;
        let deck = self.default_deck.as_ref().map_or(0, |f| f());
        Some(self.request_load(deck, &track))
    }

    /// Asks the handler to load `track` into `deck` and shows the outcome.
    pub fn request_load(&mut self, deck: usize, track: &TrackRecord) -> Result<(), LoadError> {
        let result = match self.on_load_requested.as_mut() {
            Some(handler) => handler(deck, track),
            None => return Err(LoadError::Unavailable),
        };
        self.show_load_result(&result, track);
        result
    }

    /// Queues the selected rows for analysis and returns how many were queued.
    pub fn prep_selected(&mut self, services: &mut LibraryServices) -> usize {
        let mut queued = 0;
        for row in self.selected_rows() {
            let Some(track) = self.model.row_at(row as isize).cloned() else {
                continue;
            };
            if services.prep_queue_mut().enqueue(track.id, false).is_ok() {
                queued += 1;
            }
        }
        if queued > 0 {
            self.set_status(format!("Queued {queued} for analysis"), false);
        } else {
            self.set_status("Select tracks to analyse first".to_owned(), true);
        }
        queued
    }

    /// Creates a playlist at the top level and selects it. `None` (with the reason in the status
    /// line) for a blank name or a library error.
    pub fn create_playlist(&mut self, services: &mut LibraryServices, name: &str) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() {
            self.set_status("Name the playlist first".to_owned(), true);
            return None;
        }
        match services.library().playlists().create(name, None, false) {
            Ok(id) => {
                self.playlists_changed(services);
                self.select_sidebar(services, SidebarKind::Playlist, id, "");
                self.set_status(format!("Created playlist {name}"), false);
                Some(id)
            }
            Err(error) => {
                self.set_status(format!("Couldn't create the playlist: {error}"), true);
                None
            }
        }
    }

    /// Creates a folder under `parent` (`None` = top level). `None` (with the reason in the status
    /// line) for a blank name, a parent that is not a folder, or a library error. The selection is
    /// kept, since folders are not selectable.
    pub fn new_folder(
        &mut self,
        services: &mut LibraryServices,
        parent: Option<i64>,
        name: &str,
    ) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() {
            self.set_status("Name the folder first".to_owned(), true);
            return None;
        }
        match services.library().playlists().create(name, parent, true) {
            Ok(id) => {
                self.playlists_changed(services);
                self.set_status(format!("Created folder {name}"), false);
                Some(id)
            }
            Err(error) => {
                self.set_status(format!("Couldn't create the folder: {error}"), true);
                None
            }
        }
    }

    /// Moves a playlist or folder under `new_parent` (`None` = top level) at `index` (clamped).
    /// A folder cannot go into itself or its own descendants. Returns whether it moved.
    pub fn move_playlist(
        &mut self,
        services: &mut LibraryServices,
        id: i64,
        new_parent: Option<i64>,
        index: i32,
    ) -> bool {
        match services
            .library()
            .playlists()
            .move_node(id, new_parent, index)
        {
            Ok(()) => {
                self.playlists_changed(services);
                true
            }
            Err(error) => {
                self.set_status(format!("Couldn't move the playlist: {error}"), true);
                false
            }
        }
    }

    /// Moves one playlist entry to `new_index` (clamped to the ends), keeping positions dense.
    /// Returns whether it moved.
    pub fn move_entry(
        &mut self,
        services: &mut LibraryServices,
        entry_id: i64,
        new_index: i32,
    ) -> bool {
        match services
            .library()
            .playlists()
            .move_entry(entry_id, new_index)
        {
            Ok(()) => {
                self.playlists_changed(services);
                true
            }
            Err(error) => {
                self.set_status(format!("Couldn't reorder the playlist: {error}"), true);
                false
            }
        }
    }

    /// A track dragged from the open playlist and dropped on table row `target_row`: its entry
    /// takes the place of the entry that row shows. False (nothing moves) outside a playlist, for
    /// a row that does not exist, while a search or sort hides the playlist's own order, or when
    /// the entry is already there.
    pub fn move_track_to_row(
        &mut self,
        services: &mut LibraryServices,
        track_id: i64,
        target_row: usize,
    ) -> bool {
        let (Some(playlist), Some(target)) = (
            self.playlist_order_shown(),
            self.model.row_at(target_row as isize).map(|t| t.id),
        ) else {
            return false;
        };
        let entries = match services.library().playlists().entries(playlist) {
            Ok(entries) => entries,
            Err(error) => {
                self.set_status(format!("Couldn't read the playlist: {error}"), true);
                return false;
            }
        };
        let from = entries.iter().find(|e| e.track_id == track_id);
        let to = entries.iter().find(|e| e.track_id == target);
        match (from, to) {
            (Some(from), Some(to)) if from.position != to.position => {
                self.move_entry(services, from.id, to.position)
            }
            _ => false,
        }
    }

    /// Renames the playlist shown. Returns whether it was renamed.
    pub fn rename_selected_playlist(&mut self, services: &mut LibraryServices, name: &str) -> bool {
        let Some(id) = self.selected_playlist() else {
            self.set_status("Select a playlist first".to_owned(), true);
            return false;
        };
        let name = name.trim();
        if name.is_empty() {
            self.set_status("Name the playlist first".to_owned(), true);
            return false;
        }
        match services.library().playlists().rename(id, name) {
            Ok(()) => {
                self.playlists_changed(services);
                self.set_status(format!("Renamed to {name}"), false);
                true
            }
            Err(error) => {
                self.set_status(format!("Couldn't rename the playlist: {error}"), true);
                false
            }
        }
    }

    /// Removes the playlist shown, on the second call for the same playlist: the first call only
    /// says what Delete would do. The tracks stay in the library. Returns whether it was removed.
    pub fn delete_selected_playlist(&mut self, services: &mut LibraryServices) -> bool {
        let Some(id) = self.selected_playlist() else {
            self.set_status("Select a playlist first".to_owned(), true);
            return false;
        };
        if self.delete_armed != Some(id) {
            self.delete_armed = Some(id);
            let label = self.selected.label.clone();
            self.set_status(
                format!("Delete {label}? Press Delete again (its tracks stay in the library)"),
                true,
            );
            return false;
        }
        self.delete_armed = None;
        match services.library().playlists().remove(id) {
            Ok(()) => {
                self.selected = self.nodes.first().cloned().unwrap_or(self.selected.clone());
                self.playlists_changed(services);
                self.set_status("Deleted the playlist".to_owned(), false);
                true
            }
            Err(error) => {
                self.set_status(format!("Couldn't delete the playlist: {error}"), true);
                false
            }
        }
    }

    /// Appends the selected rows to playlist `playlist_id`, skipping tracks already in it. Returns
    /// how many were added.
    pub fn add_selected_to_playlist(
        &mut self,
        services: &mut LibraryServices,
        playlist_id: i64,
    ) -> usize {
        let ids: Vec<i64> = self
            .selected_rows()
            .into_iter()
            .filter_map(|row| self.model.row_at(row as isize).map(|t| t.id))
            .collect();
        if ids.is_empty() {
            self.set_status("Select tracks to add first".to_owned(), true);
            return 0;
        }
        self.add_tracks_to_playlist(services, playlist_id, &ids)
    }

    /// Appends `track_ids` to playlist `playlist_id`, skipping tracks already in it (a drop of
    /// tracks on a sidebar playlist lands here). Returns how many were added.
    pub fn add_tracks_to_playlist(
        &mut self,
        services: &mut LibraryServices,
        playlist_id: i64,
        track_ids: &[i64],
    ) -> usize {
        let lists = services.library().playlists();
        let present: BTreeSet<i64> = match lists.entries(playlist_id) {
            Ok(entries) => entries.iter().map(|e| e.track_id).collect(),
            Err(error) => {
                self.set_status(format!("Couldn't read the playlist: {error}"), true);
                return 0;
            }
        };
        let mut added = 0;
        let mut seen = present;
        for &id in track_ids {
            if !seen.insert(id) {
                continue;
            }
            match lists.add_track(playlist_id, id, -1) {
                Ok(_) => added += 1,
                Err(error) => {
                    self.set_status(format!("Couldn't add to the playlist: {error}"), true);
                    break;
                }
            }
        }
        if added > 0 {
            self.playlists_changed(services);
            let name = find_node(&self.nodes, &|n| {
                n.kind == SidebarKind::Playlist && n.playlist_id == playlist_id
            })
            .map(|n| n.label)
            .unwrap_or_default();
            self.set_status(format!("Added {added} to {name}"), false);
        } else if !self.status_is_error {
            self.set_status("Already in the playlist".to_owned(), false);
        }
        added
    }

    /// Takes the selected rows out of the playlist shown. Returns how many entries were removed.
    pub fn remove_selected_from_playlist(&mut self, services: &mut LibraryServices) -> usize {
        let Some(id) = self.selected_playlist() else {
            self.set_status("Open a playlist to remove tracks from it".to_owned(), true);
            return 0;
        };
        let wanted: BTreeSet<i64> = self
            .selected_rows()
            .into_iter()
            .filter_map(|row| self.model.row_at(row as isize).map(|t| t.id))
            .collect();
        if wanted.is_empty() {
            self.set_status("Select tracks to remove first".to_owned(), true);
            return 0;
        }
        let lists = services.library().playlists();
        let entries = match lists.entries(id) {
            Ok(entries) => entries,
            Err(error) => {
                self.set_status(format!("Couldn't read the playlist: {error}"), true);
                return 0;
            }
        };
        let mut removed = 0;
        for entry in entries.iter().filter(|e| wanted.contains(&e.track_id)) {
            match lists.remove_entry(entry.id) {
                Ok(()) => removed += 1,
                Err(error) => {
                    self.set_status(format!("Couldn't remove from the playlist: {error}"), true);
                    break;
                }
            }
        }
        if removed > 0 {
            self.selected_rows.clear();
            self.playlists_changed(services);
            self.set_status(format!("Removed {removed} from the playlist"), false);
        }
        removed
    }

    /// The id of the playlist shown, when a playlist (not a folder or another node) is.
    pub fn selected_playlist(&self) -> Option<i64> {
        (self.selected.kind == SidebarKind::Playlist).then_some(self.selected.playlist_id)
    }

    /// Tells the other subscribers and redraws the sidebar and the table now.
    fn playlists_changed(&mut self, services: &mut LibraryServices) {
        services.notify_library_changed();
        self.refresh(services);
    }

    /// Track ids whose analysis finished since the last call. The stem auto-prepare trigger reads
    /// these on each tick.
    pub fn take_analysed(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.analysed)
    }

    /// Queues stem separation for the selected rows (the ones not already ready or preparing).
    /// When the model is not ready the error carries the status line, which the caller shows on the
    /// Library page's licence section.
    pub fn stems_selected(
        &mut self,
        services: &LibraryServices,
        host: &mut dyn StemHost,
        queue: &mut StemQueue,
    ) -> SettingsResult<usize> {
        let mut jobs: Vec<StemJob> = Vec::new();
        for row in self.selected_rows() {
            if let Some(track) = self.model.row_at(row as isize) {
                jobs.push(StemJob {
                    track_id: track.id,
                    path: PathBuf::from(&track.path),
                    content_hash: track.content_hash.clone(),
                });
            }
        }
        if jobs.is_empty() {
            self.set_status("Select tracks to prepare stems first".to_owned(), true);
            return Ok(0);
        }
        match queue.prepare(host, services.library(), &jobs) {
            Ok(queued) => {
                self.set_status(format!("Preparing stems for {queued} track(s)"), false);
                Ok(queued)
            }
            Err(error) => {
                self.set_status(error.to_string(), true);
                Err(error)
            }
        }
    }

    /// Cancels the stem jobs of the selected rows.
    pub fn cancel_stems_selected(&mut self, queue: &mut StemQueue) -> usize {
        let mut ids: Vec<i64> = Vec::new();
        for row in self.selected_rows() {
            if let Some(track) = self.model.row_at(row as isize) {
                ids.push(track.id);
            }
        }
        for id in &ids {
            queue.cancel(*id);
        }
        if !ids.is_empty() {
            self.set_status(format!("Cancelled stems for {} track(s)", ids.len()), false);
        }
        ids.len()
    }

    /// Imports an iTunes or Rekordbox XML file and reports the outcome in the status line.
    pub fn import_from(
        &mut self,
        services: &mut LibraryServices,
        xml: &Path,
    ) -> Result<ImportReport, LibraryError> {
        match services.import_file(xml) {
            Ok(report) => {
                self.set_status(import_summary(&report), report.errors > 0);
                Ok(report)
            }
            Err(error) => {
                self.set_status(format!("Import failed: {error}"), true);
                Err(error)
            }
        }
    }

    /// The browser's tick: pumps the services, applies what changed since the last tick (sidebar
    /// rebuild, pending search, queue updates) and polls the reference key.
    pub fn flush_pending(&mut self, services: &mut LibraryServices) {
        services.pump();
        loop {
            match self.events.try_recv() {
                Ok(LibraryEvent::LibraryChanged) => self.rebuild_sidebar = true,
                Ok(LibraryEvent::PrepEntryChanged(entry)) => {
                    if entry.status == PrepStatus::Done {
                        self.analysed.push(entry.track_id);
                    }
                    if self.on_queue() || entry.status == PrepStatus::Done {
                        self.stale = true;
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if self.rebuild_sidebar {
            self.refresh(services);
        } else if self.search_pending || self.stale {
            self.apply_query(services);
        }
        // Rows that could not be read while painting are reported here, once per change.
        let page_error = self.model.source_error();
        if page_error != self.shown_page_error {
            let was_failing = !self.shown_page_error.is_empty();
            self.shown_page_error = page_error.clone();
            if !page_error.is_empty() {
                self.set_status(format!("cannot read rows: {page_error}"), true);
            } else if was_failing {
                self.apply_query(services);
            }
        }
        let reference = self.reference_key.as_ref().map(|f| f());
        if let Some(reference) = reference {
            if reference != self.shown_reference {
                self.shown_reference = reference.clone();
                self.model.set_reference_camelot(&reference);
            }
        }
    }

    /// Rebuilds the sidebar from the library, keeps the selected node when it still exists, and
    /// reruns the query.
    pub fn refresh(&mut self, services: &LibraryServices) {
        let sidebar = build_sidebar(services.library());
        self.sidebar_error = sidebar.error.as_ref().map(ToString::to_string);
        self.nodes = sidebar.nodes;
        self.rebuild_sidebar = false;
        let wanted = self.selected.clone();
        let still_there = find_node(&self.nodes, &|n| {
            n.kind == wanted.kind && n.playlist_id == wanted.playlist_id && n.path == wanted.path
        });
        self.selected = still_there
            .or_else(|| self.nodes.first().cloned())
            .unwrap_or(wanted);
        self.apply_query(services);
    }

    /// The open playlist's id while the table shows it in its own order, which is while no search
    /// text or header sort is applied. Only then do rows line up with entries and can be reordered.
    pub fn playlist_order_shown(&self) -> Option<i64> {
        (self.search_text.trim().is_empty() && self.sort_field.is_empty())
            .then(|| self.selected_playlist())
            .flatten()
    }

    fn on_queue(&self) -> bool {
        self.selected.kind == SidebarKind::PrepQueue
    }

    /// Replaces the table's source with the current query and updates the status line.
    fn apply_query(&mut self, services: &LibraryServices) {
        self.search_pending = false;
        self.stale = false;
        let playlist_order = self.playlist_order_shown();
        let source: Box<dyn TrackSource> = if self.on_queue() {
            let entries = services.prep_queue().list().unwrap_or_default();
            Box::new(PrepQueueSource::new(services.library_handle(), entries))
        } else if let Some(entries) =
            playlist_order.and_then(|id| services.library().playlists().entries(id).ok())
        {
            let ids = entries.iter().map(|e| e.track_id).collect();
            Box::new(PlaylistSource::new(services.library_handle(), ids))
        } else {
            Box::new(SearchTrackSource::new(
                services.search(),
                &self.selected.scope_query(),
                &self.search_text,
                &self.sort_field,
                self.sort_descending,
            ))
        };
        self.model.set_source(source);
        let count = self.model.num_rows();
        self.selected_rows.retain(|&row| row < count);
        let error = self.model.source_error();
        if !error.is_empty() {
            self.set_status(error, true);
        } else if let Some(sidebar_error) = self.sidebar_error.clone() {
            self.set_status(format!("Sidebar unavailable: {sidebar_error}"), true);
        } else {
            let text = if self.on_queue() {
                format!("{count} in the prep queue")
            } else {
                format!("{count} tracks")
            };
            self.set_status(text, false);
        }
    }

    fn set_status(&mut self, text: String, is_error: bool) {
        self.status = text;
        self.status_is_error = is_error;
    }

    fn show_load_result(&mut self, result: &Result<(), LoadError>, track: &TrackRecord) {
        let name = Path::new(&track.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match result {
            Err(error) => self.set_status(format!("Couldn't load {name}: {error}"), true),
            Ok(()) => self.set_status(format!("Loaded {name}"), false),
        }
    }
}

/// Depth-first search for the first node that matches, skipping nothing.
fn find_node(nodes: &[SidebarNode], matches: &dyn Fn(&SidebarNode) -> bool) -> Option<SidebarNode> {
    for node in nodes {
        if matches(node) {
            return Some(node.clone());
        }
        if let Some(found) = find_node(&node.children, matches) {
            return Some(found);
        }
    }
    None
}
