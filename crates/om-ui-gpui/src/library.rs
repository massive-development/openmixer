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
use om_core::jobs::JobHandle;
use om_engine::DjEngine;
use om_library::{PrepStatus, TrackRecord};
use om_ui::library::{
    columns, describe_track_drag, parse_track_drag, LibraryBrowser, LibraryServices, LoadError,
    MixLevel, SearchFeed, TrackLoader, STEMS_COLUMN,
};
use om_ui::settings::{LibraryControl, SettingsError, SettingsResult};
use om_ui::sidebar::SidebarNode;
use om_ui::stems::{ModelState, StemHost, StemJob, StemQueue};
use om_ui::widgets::deck::TrackInfo;
use std::path::PathBuf;

/// Auto-prepare: the tracks whose analysis finished this tick are queued for stems when the
/// preference is on (see `StemQueue::on_analysed`). Failures are logged, not shown.
fn run_auto_prepare(analysed: Vec<i64>, hub: &mut LibraryHub) {
    if let Some(host) = hub.stem_host.as_deref_mut() {
        for track_id in analysed {
            if let Err(error) =
                hub.stems
                    .on_analysed(host, hub.services.library(), track_id, PrepStatus::Done)
            {
                eprintln!("openmixer: stems: {error}");
            }
        }
    }
}

/// The services and the loader, shared by the browser's handler and the window. The stems host
/// is set by the app once the stems are attached (`None` before, or without a library file).
pub struct LibraryHub {
    pub services: LibraryServices,
    pub loader: TrackLoader,
    pub stems: StemQueue,
    pub stem_host: Option<Box<dyn StemHost>>,
    /// The browser's search text, published for the automix queue's current-search source.
    pub search_feed: SearchFeed,
}

pub type SharedHub = Rc<RefCell<LibraryHub>>;

/// The settings page's view of the library: the window's shared services, so the page's watch
/// folders, rescan and import reach the same library the browser shows.
pub struct HubLibrary(pub SharedHub);

/// The stems page's view of the stems host in the hub, so the page and the browser act on the same
/// host. Without a host every action says the stems are not available.
pub struct HubStems(pub SharedHub);

impl StemHost for HubStems {
    fn model_state(&self) -> ModelState {
        self.0.borrow().stem_host.as_deref().map_or_else(
            || ModelState::NotInstalled("the library is not available".into()),
            |host| host.model_state(),
        )
    }

    fn licence(&self) -> Option<(String, String)> {
        self.0
            .borrow()
            .stem_host
            .as_deref()
            .and_then(|h| h.licence())
    }

    fn accept_and_download(&mut self) -> SettingsResult<()> {
        match self.0.borrow_mut().stem_host.as_deref_mut() {
            Some(host) => host.accept_and_download(),
            None => Err(SettingsError::StemsUnavailable),
        }
    }

    fn cancel_download(&mut self) {
        if let Some(host) = self.0.borrow_mut().stem_host.as_deref_mut() {
            host.cancel_download();
        }
    }

    fn separator(&self) -> (String, i32) {
        self.0
            .borrow()
            .stem_host
            .as_deref()
            .map_or_else(|| ("none".to_owned(), 0), |h| h.separator())
    }

    fn cache_bytes(&self) -> u64 {
        self.0
            .borrow()
            .stem_host
            .as_deref()
            .map_or(0, |h| h.cache_bytes())
    }

    fn clear_cache(&mut self) -> SettingsResult<usize> {
        match self.0.borrow_mut().stem_host.as_deref_mut() {
            Some(host) => host.clear_cache(),
            None => Ok(0),
        }
    }

    fn set_max_cache_bytes(&mut self, bytes: u64) {
        if let Some(host) = self.0.borrow_mut().stem_host.as_deref_mut() {
            host.set_max_cache_bytes(bytes);
        }
    }

    fn auto_prepare(&self) -> bool {
        self.0
            .borrow()
            .stem_host
            .as_deref()
            .is_some_and(|h| h.auto_prepare())
    }

    fn set_auto_prepare(&mut self, on: bool) {
        if let Some(host) = self.0.borrow_mut().stem_host.as_deref_mut() {
            host.set_auto_prepare(on);
        }
    }

    fn submit(&mut self, job: StemJob) -> SettingsResult<JobHandle> {
        match self.0.borrow_mut().stem_host.as_deref_mut() {
            Some(host) => host.submit(job),
            None => Err(SettingsError::StemsUnavailable),
        }
    }
}

impl LibraryControl for HubLibrary {
    fn available(&self) -> bool {
        LibraryControl::available(&self.0.borrow().services)
    }

    fn folders(&self) -> SettingsResult<Vec<PathBuf>> {
        LibraryControl::folders(&self.0.borrow().services)
    }

    fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        LibraryControl::add_watch_folder(&mut self.0.borrow_mut().services, folder)
    }

    fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        LibraryControl::remove_watch_folder(&mut self.0.borrow_mut().services, folder)
    }

    fn set_scan_interval_minutes(&mut self, minutes: i32) {
        LibraryControl::set_scan_interval_minutes(&mut self.0.borrow_mut().services, minutes);
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
        let stems = StemQueue::new(om_ui::stems::watermark_of(services.library()));
        Rc::new(RefCell::new(Self {
            services,
            loader,
            stems,
            search_feed: SearchFeed::default(),
            stem_host: None,
        }))
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
    /// How strongly the row stands out: key and tempo both fit, key only, or neither.
    pub mix: MixLevel,
    pub selected: bool,
    /// The deck this track is loaded into, if any (the row takes that deck's accent).
    pub on_deck: Option<usize>,
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

/// A playlist name being typed in the search box's place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEntry {
    /// `Some(id)` renames that playlist; `None` names a new one.
    pub rename: Option<i64>,
    pub text: String,
}

impl NameEntry {
    /// What the box shows while the name is typed.
    pub fn prompt(&self) -> String {
        let what = if self.rename.is_some() {
            "Rename playlist"
        } else {
            "New playlist"
        };
        format!("{what}: {}|", self.text)
    }
}

pub struct LibraryPanel {
    hub: SharedHub,
    engine: Arc<DjEngine>,
    pub browser: LibraryBrowser,
    /// Typing goes to the search box (set by a click on it or by `/`).
    pub search_active: bool,
    /// A playlist name being typed; takes the keys before the search box does.
    pub name_entry: Option<NameEntry>,
    /// Set when a stems action needs the model first: the window opens the Library settings page.
    licence_request: bool,
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
        browser.search_feed = Some(Arc::clone(&hub.borrow().search_feed));
        let load_hub = Rc::clone(&hub);
        let load_engine = Arc::clone(&engine);
        browser.on_load_requested = Some(Box::new(move |deck, track| {
            let mut h = load_hub.borrow_mut();
            let LibraryHub {
                services, loader, ..
            } = &mut *h;
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
            name_entry: None,
            licence_request: false,
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
            run_auto_prepare(self.browser.take_analysed(), &mut h);
            let LibraryHub {
                services, loader, ..
            } = &mut *h;
            loader.pump(&self.engine, services);
        }
        let (reference, bpm) = {
            let h = self.hub.borrow();
            (
                h.loader.reference_camelot(&self.engine),
                h.loader.reference_bpm(&self.engine),
            )
        };
        if reference != self.shown_reference {
            self.shown_reference.clone_from(&reference);
            self.browser.model.set_reference_camelot(&reference);
        }
        self.browser.model.set_reference_bpm(bpm);
    }

    /// Rows in the table.
    /// True while the search field takes typed keys.
    pub fn is_typing(&self) -> bool {
        self.search_active || self.name_entry.is_some()
    }

    /// Starts naming a new playlist.
    pub fn begin_new_playlist(&mut self) {
        self.search_active = false;
        self.name_entry = Some(NameEntry {
            rename: None,
            text: String::new(),
        });
    }

    /// Starts renaming the playlist shown; the status line says so when none is.
    pub fn begin_rename_playlist(&mut self) {
        match self.browser.selected_playlist() {
            Some(id) => {
                self.search_active = false;
                self.name_entry = Some(NameEntry {
                    rename: Some(id),
                    text: self.browser.selected_node().label.clone(),
                });
            }
            None => {
                let mut h = self.hub.borrow_mut();
                self.browser.rename_selected_playlist(&mut h.services, "");
            }
        }
    }

    /// Deletes the playlist shown, on the second press (see the browser).
    pub fn delete_playlist(&mut self) {
        let mut h = self.hub.borrow_mut();
        self.browser.delete_selected_playlist(&mut h.services);
    }

    /// Takes the selected rows out of the playlist shown.
    pub fn remove_selected_from_playlist(&mut self) -> usize {
        let mut h = self.hub.borrow_mut();
        self.browser.remove_selected_from_playlist(&mut h.services)
    }

    /// A drop on a sidebar playlist: adds the dragged track, or the whole selection when the
    /// dragged track is part of it. `None` when the drag was not a library track.
    pub fn drop_on_playlist(&mut self, playlist_id: i64, description: &str) -> Option<usize> {
        let id = parse_track_drag(description)?;
        let mut h = self.hub.borrow_mut();
        let selected: Vec<i64> = self
            .browser
            .selected_rows()
            .into_iter()
            .filter_map(|row| self.browser.model.row_at(row as isize).map(|t| t.id))
            .collect();
        let ids = if selected.contains(&id) {
            selected
        } else {
            vec![id]
        };
        Some(
            self.browser
                .add_tracks_to_playlist(&mut h.services, playlist_id, &ids),
        )
    }

    /// Typed keys for the playlist name: text edits it, Enter creates or renames, Escape cancels.
    fn handle_name_key(&mut self, key: &str, text: Option<&str>) -> KeyOutcome {
        let Some(entry) = self.name_entry.as_mut() else {
            return KeyOutcome::Ignored;
        };
        match key {
            "escape" => {
                self.name_entry = None;
            }
            "enter" => {
                let entry = entry.clone();
                let mut h = self.hub.borrow_mut();
                let done = match entry.rename {
                    Some(_) => self
                        .browser
                        .rename_selected_playlist(&mut h.services, &entry.text),
                    None => self
                        .browser
                        .create_playlist(&mut h.services, &entry.text)
                        .is_some(),
                };
                // A refused name stays open so it can be fixed; the status line says why.
                if done {
                    self.name_entry = None;
                }
            }
            "backspace" => {
                entry.text.pop();
            }
            _ => match text {
                Some(typed) if !typed.chars().any(char::is_control) => entry.text.push_str(typed),
                _ => return KeyOutcome::Ignored,
            },
        }
        KeyOutcome::Edited
    }

    pub fn row_count(&self) -> usize {
        self.browser.model.num_rows()
    }

    /// The row at `row`, with its cells, tint and selection. `None` outside the list.
    pub fn row(&mut self, row: usize) -> Option<RowView> {
        let track: TrackRecord = self.browser.model.row_at(row as isize)?.clone();
        let stems = self.stems_label(track.id);
        let cells = columns()
            .iter()
            .map(|c| {
                if c.id == STEMS_COLUMN {
                    stems.clone()
                } else {
                    self.browser.model.cell_text(row as isize, c.id)
                }
            })
            .collect();
        Some(RowView {
            row,
            track_id: track.id,
            cells,
            compatible: self.browser.model.is_compatible(&track),
            mix: self.browser.model.mix_level(&track),
            selected: self.browser.selected_rows().contains(&row),
            on_deck: (0..self.engine.num_decks())
                .find(|&deck| self.loaded_track_id(deck) == Some(track.id)),
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

    /// Loads the audio file at `path` into `deck` through the shared loader (see
    /// `TrackLoader::load_path`). A playing deck refuses unless `force`.
    pub fn load_path(
        &mut self,
        deck: usize,
        path: &std::path::Path,
        force: bool,
    ) -> Result<(), LoadError> {
        let mut h = self.hub.borrow_mut();
        let LibraryHub {
            services, loader, ..
        } = &mut *h;
        loader.load_path(&self.engine, services, deck, path, force)
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

    /// The explicit 'Load stems' / 'Stems off' action for the loaded track of `deck`. `Ok(true)`
    /// when the deck now plays from stems.
    pub fn toggle_stems(&mut self, deck: usize) -> Result<bool, LoadError> {
        let mut hub = self.hub.borrow_mut();
        let hub = &mut *hub;
        hub.loader.toggle_stems(&self.engine, &hub.services, deck)
    }

    /// Why the stems of `deck` did not load with its last track, for the deck card's caption.
    pub fn stem_note(&self, deck: usize) -> Option<String> {
        self.hub.borrow().loader.stem_note(deck).map(str::to_owned)
    }

    /// The stems indicator of a track: None, Preparing n%, Ready or Failed.
    pub fn stems_label(&self, track_id: i64) -> String {
        let h = self.hub.borrow();
        match h.stem_host.as_deref() {
            None => "None".to_owned(),
            Some(host) => {
                let (sep, version) = host.separator();
                h.stems
                    .label(h.services.library(), track_id, (&sep, version))
                    .text()
            }
        }
    }

    /// Queues stem separation for the selected rows. The error (the model is not ready, or the
    /// stems are not available) carries the status line the Library page shows.
    pub fn stems_selected(&mut self) -> SettingsResult<usize> {
        let mut h = self.hub.borrow_mut();
        let hub = &mut *h;
        let Some(host) = hub.stem_host.as_deref_mut() else {
            return Err(SettingsError::StemsUnavailable);
        };
        self.browser
            .stems_selected(&hub.services, host, &mut hub.stems)
    }

    /// The "Prepare stems" button and the `p` key. When the model is not ready, the browser's status
    /// line says why and the window is asked to open the Library settings page.
    pub fn prepare_stems(&mut self) {
        if let Err(SettingsError::StemsNotReady(_)) = self.stems_selected() {
            self.licence_request = true;
        }
    }

    /// True once after a stems action needed the Library settings page; the window opens it.
    pub fn take_licence_request(&mut self) -> bool {
        std::mem::take(&mut self.licence_request)
    }

    /// Cancels the stem jobs of the selected rows.
    pub fn cancel_stems_selected(&mut self) -> usize {
        let mut h = self.hub.borrow_mut();
        self.browser.cancel_stems_selected(&mut h.stems)
    }

    /// Stem jobs queued or running; the toolbar offers Cancel only while this is above zero.
    pub fn stems_in_flight(&self) -> usize {
        self.hub.borrow().stems.in_flight()
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
        if self.name_entry.is_some() {
            return self.handle_name_key(key, text);
        }
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
            "p" => {
                self.prepare_stems();
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

    /// The library id of the track loaded into `deck`, if any (the stem pads watch it for changes).
    pub fn loaded_track_id(&self, deck: usize) -> Option<i64> {
        self.hub.borrow().loader.loaded_track(deck).map(|t| t.id)
    }

    /// Hub for the window's own reads (the smoke check and tests).
    pub fn hub(&self) -> &SharedHub {
        &self.hub
    }
}
