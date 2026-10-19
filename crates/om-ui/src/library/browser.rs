//! The library browser's logic: the track table, the sidebar selection, the search box (applied on
//! the next tick, not per keystroke), the status line, load requests to the decks, prep queueing and
//! XML import. The browser does not own the services: each action takes them, so the owner keeps the
//! one mutable handle. Port of `src/ui/LibraryBrowser.{h,cpp}` (logic only; the view is in gpui).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};

use om_library::{ImportReport, PrepStatus, TrackRecord};

use super::control::import_summary;
use super::services::{LibraryEvent, LibraryServices};
use super::track_source::{PrepQueueSource, SearchTrackSource, TrackSource};
use super::track_table::TrackTableModel;
use crate::sidebar::{build_sidebar, SidebarKind, SidebarNode};

/// Placeholder shown in the empty search box.
pub const SEARCH_PLACEHOLDER: &str =
    "Search: artist:name bpm:120-128 key:~8A -genre:techno sort:-added";

/// Called with the deck (0-based) and the track when the user asks to load it. An error is shown in
/// the status line.
pub type LoadHandler = Box<dyn FnMut(usize, &TrackRecord) -> Result<(), String>>;

pub struct LibraryBrowser {
    pub model: TrackTableModel,
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
    shown_reference: String,
    /// The page error last shown in the status line; empty when none.
    shown_page_error: String,
    selected_rows: BTreeSet<usize>,
    events: Receiver<LibraryEvent>,
}

impl LibraryBrowser {
    /// Builds the sidebar from the library and shows all tracks.
    pub fn new(services: &mut LibraryServices, num_decks: usize) -> Self {
        let events = services.subscribe();
        let nodes = build_sidebar(services.library());
        let selected = nodes.first().cloned().unwrap_or_else(|| SidebarNode {
            kind: SidebarKind::AllTracks,
            label: "All tracks".to_owned(),
            playlist_id: 0,
            path: String::new(),
            children: Vec::new(),
        });
        let mut browser = Self {
            model: TrackTableModel::new(),
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
            shown_reference: String::new(),
            shown_page_error: String::new(),
            selected_rows: BTreeSet::new(),
            events,
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

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn status_is_error(&self) -> bool {
        self.status_is_error
    }

    /// Replaces the search text and applies it now.
    pub fn set_search_text(&mut self, services: &LibraryServices, text: &str) {
        self.search_text = text.to_owned();
        self.search_pending = false;
        self.apply_query(services);
    }

    /// The user typed. The query runs on the next [`Self::flush_pending`], not per keystroke.
    pub fn type_search(&mut self, text: &str) {
        self.search_text = text.to_owned();
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
    pub fn activate_row(&mut self, row: usize) -> Option<Result<(), String>> {
        let track = self.model.activate(row as isize)?;
        let deck = self.default_deck.as_ref().map_or(0, |f| f());
        Some(self.request_load(deck, &track))
    }

    /// Asks the handler to load `track` into `deck` and shows the outcome.
    pub fn request_load(&mut self, deck: usize, track: &TrackRecord) -> Result<(), String> {
        let result = match self.on_load_requested.as_mut() {
            Some(handler) => handler(deck, track),
            None => return Err("loading is not available".to_owned()),
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

    /// Imports an iTunes or Rekordbox XML file and reports the outcome in the status line.
    pub fn import_from(
        &mut self,
        services: &mut LibraryServices,
        xml: &Path,
    ) -> Result<ImportReport, String> {
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
        self.nodes = build_sidebar(services.library());
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

    fn on_queue(&self) -> bool {
        self.selected.kind == SidebarKind::PrepQueue
    }

    /// Replaces the table's source with the current query and updates the status line.
    fn apply_query(&mut self, services: &LibraryServices) {
        self.search_pending = false;
        self.stale = false;
        let source: Box<dyn TrackSource> = if self.on_queue() {
            let entries = services.prep_queue().list().unwrap_or_default();
            Box::new(PrepQueueSource::new(services.library_handle(), entries))
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
        if error.is_empty() {
            let text = if self.on_queue() {
                format!("{count} in the prep queue")
            } else {
                format!("{count} tracks")
            };
            self.set_status(text, false);
        } else {
            self.set_status(error, true);
        }
    }

    fn set_status(&mut self, text: String, is_error: bool) {
        self.status = text;
        self.status_is_error = is_error;
    }

    fn show_load_result(&mut self, result: &Result<(), String>, track: &TrackRecord) {
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
