//! Library glue for the window, with no gpui types: the shared services and track loader, and the
//! browser's actions as plain calls. The window's events, the deck drop target and the `--smoke`
//! check all go through [`LibraryPanel`], so the behaviour is testable without a window.
//!
//! The services and the loader sit in one [`SharedHub`] that the browser's load handler reaches.
//! The browser itself is a separate field, so nothing borrows the hub while the browser runs.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use om_analysis::WaveformData;
use om_engine::DjEngine;
use om_library::TrackRecord;
use om_ui::library::{
    columns, describe_track_drag, parse_track_drag, LibraryBrowser, LibraryServices, LoadError,
    TrackLoader,
};
use om_ui::settings::{LibraryControl, SettingsResult};
use om_ui::sidebar::SidebarNode;
use om_ui::widgets::deck::TrackInfo;
use std::path::PathBuf;

/// The services and the loader, shared by the browser's handler and the window.
pub struct LibraryHub {
    pub services: LibraryServices,
    pub loader: TrackLoader,
}

pub type SharedHub = Rc<RefCell<LibraryHub>>;

/// The settings page's view of the library: the window's shared services, so the page's watch
/// folders, rescan and import reach the same library the browser shows.
pub struct HubLibrary(pub SharedHub);

impl LibraryControl for HubLibrary {
    fn available(&self) -> bool {
        LibraryControl::available(&self.0.borrow().services)
    }

    fn folders(&self) -> Vec<PathBuf> {
        LibraryControl::folders(&self.0.borrow().services)
    }

    fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        LibraryControl::add_watch_folder(&mut self.0.borrow_mut().services, folder)
    }

    fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        LibraryControl::remove_watch_folder(&mut self.0.borrow_mut().services, folder)
    }

    fn rescan(&mut self) -> SettingsResult<()> {
        LibraryControl::rescan(&mut self.0.borrow_mut().services)
    }

    fn import_xml(&mut self, xml: PathBuf) -> SettingsResult<String> {
        LibraryControl::import_xml(&mut self.0.borrow_mut().services, xml)
    }
}

impl LibraryHub {
    /// Wraps the services with a loader for `num_decks` decks.
    pub fn shared(mut services: LibraryServices, num_decks: usize) -> SharedHub {
        let loader = TrackLoader::new(&mut services, num_decks);
        Rc::new(RefCell::new(Self { services, loader }))
    }
}

/// What the table shows for one row.
#[derive(Debug, Clone, PartialEq)]
pub struct RowView {
    /// Position in the table.
    pub row: usize,
    pub track_id: i64,
    /// One string per column of [`columns`], in order.
    pub cells: Vec<String>,
    /// Harmonically compatible with the reference key (the Camelot tint).
    pub compatible: bool,
    pub selected: bool,
    /// Drag payload for the row (the library track id).
    pub drag: String,
}

/// One sidebar line: the node and how deep it sits.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarLine {
    pub depth: usize,
    pub node: SidebarNode,
    pub selected: bool,
}

/// What a key press did to the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Not a key the browser uses.
    Ignored,
    /// The search text or the search mode changed.
    Edited,
    /// The selection moved.
    Moved,
    /// Enter: the selected row was activated (loaded into the suggested deck).
    Activated,
}

pub struct LibraryPanel {
    hub: SharedHub,
    engine: Arc<DjEngine>,
    pub browser: LibraryBrowser,
    /// Typing goes to the search box (set by a click on it or by `/`).
    pub search_active: bool,
    /// Column and direction of the last header click (`true` = ascending).
    sort: Option<(i32, bool)>,
    /// The first table row on screen.
    pub table_top: usize,
    /// Rows that fit in the table, as the window last laid it out.
    pub visible_rows: usize,
    shown_reference: String,
}

impl LibraryPanel {
    /// Builds the browser over the hub's services. Loads go through the hub's loader into the
    /// deck the browser suggests (an idle deck, else deck 0).
    pub fn new(hub: SharedHub, engine: Arc<DjEngine>) -> Self {
        let num_decks = engine.num_decks();
        let mut browser = {
            let mut h = hub.borrow_mut();
            LibraryBrowser::new(&mut h.services, num_decks)
        };
        let load_hub = Rc::clone(&hub);
        let load_engine = Arc::clone(&engine);
        browser.on_load_requested = Some(Box::new(move |deck, track| {
            let mut h = load_hub.borrow_mut();
            let LibraryHub { services, loader } = &mut *h;
            loader.load_track(&load_engine, services, deck, track, false)
        }));
        let suggest_hub = Rc::clone(&hub);
        let suggest_engine = Arc::clone(&engine);
        browser.default_deck = Some(Box::new(move || {
            suggest_hub.borrow().loader.suggested_deck(&suggest_engine)
        }));
        Self {
            hub,
            engine,
            browser,
            search_active: false,
            sort: None,
            table_top: 0,
            visible_rows: 10,
            shown_reference: String::new(),
        }
    }

    /// The browser's tick: pumps the services and the loader, applies what changed, and follows
    /// the reference key (the key of the playing or first loaded deck).
    pub fn tick(&mut self) {
        {
            let mut h = self.hub.borrow_mut();
            self.browser.flush_pending(&mut h.services);
            let LibraryHub { services, loader } = &mut *h;
            loader.pump(&self.engine, services);
        }
        let reference = self.hub.borrow().loader.reference_camelot(&self.engine);
        if reference != self.shown_reference {
            self.shown_reference.clone_from(&reference);
            self.browser.model.set_reference_camelot(&reference);
        }
    }

    /// Rows in the table.
    pub fn row_count(&self) -> usize {
        self.browser.model.num_rows()
    }

    /// The row at `row`, with its cells, tint and selection. `None` outside the list.
    pub fn row(&mut self, row: usize) -> Option<RowView> {
        let track: TrackRecord = self.browser.model.row_at(row as isize)?.clone();
        let cells = columns()
            .iter()
            .map(|c| self.browser.model.cell_text(row as isize, c.id))
            .collect();
        Some(RowView {
            row,
            track_id: track.id,
            cells,
            compatible: self.browser.model.is_compatible(&track),
            selected: self.browser.selected_rows().contains(&row),
            drag: describe_track_drag(track.id),
        })
    }

    /// The sidebar, flattened depth-first, with the selected node marked.
    pub fn sidebar_lines(&self) -> Vec<SidebarLine> {
        fn walk(
            nodes: &[SidebarNode],
            depth: usize,
            selected: &SidebarNode,
            out: &mut Vec<SidebarLine>,
        ) {
            for node in nodes {
                let is_selected = node.kind == selected.kind
                    && node.playlist_id == selected.playlist_id
                    && node.path == selected.path;
                out.push(SidebarLine {
                    depth,
                    node: node.clone(),
                    selected: is_selected,
                });
                walk(&node.children, depth + 1, selected, out);
            }
        }
        let mut out = Vec::new();
        walk(
            self.browser.sidebar_nodes(),
            0,
            self.browser.selected_node(),
            &mut out,
        );
        out
    }

    /// Selects a sidebar node. Groups are not selectable; returns whether the node was selected.
    pub fn select_sidebar(&mut self, node: &SidebarNode) -> bool {
        let h = self.hub.borrow();
        self.browser
            .select_sidebar(&h.services, node.kind, node.playlist_id, &node.path)
    }

    pub fn select_row(&mut self, row: usize) {
        self.browser.select_row(row);
    }

    /// Extends the selection from the first selected row (or `row`) to `row`.
    pub fn extend_selection(&mut self, row: usize) {
        let anchor = self.browser.selected_row().unwrap_or(row);
        self.browser.select_range(anchor, row);
    }

    /// The column and direction of the last header click, for the header marker.
    pub fn sort_indicator(&self) -> Option<(i32, bool)> {
        self.sort
    }

    /// Header click: sorts by the column, flipping direction on a second click on the same one.
    pub fn sort_by_column(&mut self, column_id: i32) -> bool {
        let forwards = match self.sort {
            Some((column, forwards)) if column == column_id => !forwards,
            _ => true,
        };
        self.sort = Some((column_id, forwards));
        let h = self.hub.borrow();
        self.browser.header_sort(&h.services, column_id, forwards)
    }

    /// Activates a row (double-click): loads it into the suggested deck.
    pub fn activate_row(&mut self, row: usize) -> Option<Result<(), LoadError>> {
        self.browser.activate_row(row)
    }

    /// The "Load to deck" buttons: loads the first selected row into `deck`.
    pub fn load_selected(&mut self, deck: usize) {
        self.browser.click_load(deck);
    }

    /// Loads a track by id into `deck` and reports the outcome in the browser's status line.
    pub fn load_track_id(&mut self, deck: usize, track_id: i64) -> Result<(), LoadError> {
        let track = {
            let h = self.hub.borrow();
            h.services
                .library()
                .tracks()
                .find_by_id(track_id)?
                .ok_or(LoadError::UnknownTrack(track_id))?
        };
        self.browser.request_load(deck, &track)
    }

    /// A drop on deck `deck`: loads the library track the description names. `None` when the drag
    /// was not a library track.
    pub fn drop_on_deck(
        &mut self,
        deck: usize,
        description: &str,
    ) -> Option<Result<(), LoadError>> {
        let id = parse_track_drag(description)?;
        Some(self.load_track_id(deck, id))
    }

    /// Queues the selected rows for analysis.
    pub fn prep_selected(&mut self) -> usize {
        let mut h = self.hub.borrow_mut();
        self.browser.prep_selected(&mut h.services)
    }

    /// Keyboard input for the browser. While the search box is active, printable text edits the
    /// query (applied on the next tick), Backspace deletes, Enter and Escape leave the box. Else
    /// `/` opens the box, arrows move the selection and Enter activates it.
    /// `text` is the typed character (if any); `key` is the key name.
    pub fn handle_key(&mut self, key: &str, text: Option<&str>) -> KeyOutcome {
        if self.search_active {
            return match key {
                "enter" => {
                    self.search_active = false;
                    let query = self.browser.search_text().to_owned();
                    let h = self.hub.borrow();
                    self.browser.set_search_text(&h.services, &query);
                    KeyOutcome::Edited
                }
                "escape" => {
                    self.search_active = false;
                    let h = self.hub.borrow();
                    self.browser.set_search_text(&h.services, "");
                    KeyOutcome::Edited
                }
                "backspace" => {
                    let mut text = self.browser.search_text().to_owned();
                    text.pop();
                    self.browser.type_search(&text);
                    KeyOutcome::Edited
                }
                _ => match text {
                    Some(typed) if !typed.chars().any(char::is_control) => {
                        let mut query = self.browser.search_text().to_owned();
                        query.push_str(typed);
                        self.browser.type_search(&query);
                        KeyOutcome::Edited
                    }
                    _ => KeyOutcome::Ignored,
                },
            };
        }
        match key {
            "/" => {
                self.search_active = true;
                KeyOutcome::Edited
            }
            "up" | "down" => {
                let count = self.row_count();
                if count == 0 {
                    return KeyOutcome::Ignored;
                }
                let current = self.browser.selected_row();
                let next = match (current, key) {
                    (None, _) => 0,
                    (Some(row), "up") => row.saturating_sub(1),
                    (Some(row), _) => (row + 1).min(count - 1),
                };
                self.browser.select_row(next);
                self.keep_visible(next);
                KeyOutcome::Moved
            }
            "enter" => match self.browser.selected_row() {
                Some(row) => {
                    let _ = self.activate_row(row);
                    KeyOutcome::Activated
                }
                None => KeyOutcome::Ignored,
            },
            _ => KeyOutcome::Ignored,
        }
    }

    /// Moves the table's top row so `row` is on screen.
    pub fn keep_visible(&mut self, row: usize) {
        let visible = self.visible_rows.max(1);
        if row < self.table_top {
            self.table_top = row;
        } else if row >= self.table_top + visible {
            self.table_top = row + 1 - visible;
        }
    }

    /// Scrolls the table by whole rows (negative = up), clamped to the list.
    pub fn scroll_rows(&mut self, rows: isize) {
        let count = self.row_count();
        let max_top = count.saturating_sub(self.visible_rows.max(1));
        let top = self.table_top as isize + rows;
        self.table_top = top.clamp(0, max_top as isize) as usize;
    }

    /// The loaded track's header text and the grid BPM it shows.
    pub fn track_info(&self, deck: usize) -> TrackInfo {
        self.hub.borrow().loader.track_info(deck)
    }

    /// The analysed waveform for `deck`, once the track's cache is known.
    pub fn waveform(&self, deck: usize) -> Option<Arc<WaveformData>> {
        self.hub.borrow().loader.waveform(deck).cloned()
    }

    /// Whether a track is loaded into `deck` (through the loader, which owns the record).
    pub fn has_track(&self, deck: usize) -> bool {
        self.hub.borrow().loader.loaded_track(deck).is_some()
    }

    /// Hub for the window's own reads (the smoke check and tests).
    pub fn hub(&self) -> &SharedHub {
        &self.hub
    }
}
