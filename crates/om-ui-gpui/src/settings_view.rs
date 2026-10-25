//! The settings overlay: a card with the five tabs over the window, opened by the Settings
//! button in the header and closed by its Close button, Escape or a click on the scrim. Draws the
//! card and its pages. The logic (what each control does, what is
//! stored, what the services are asked to do) lives in om-ui's `settings` module and in the methods
//! of [`SettingsState`] here; this file draws it and forwards clicks.
//!
//! Pages: Audio edits the output model. Appearance picks a theme, plugin themes included. MIDI
//! drives the running MIDI service: inputs, the map file, MIDI learn. Library drives the library:
//! watch folders, the rescan timer, rescan and import. Plugins lists the loaded plugins and loads
//! or unloads the built-ins. A page that needs a path opens the text field at the foot of the card:
//! type, Enter applies, Escape cancels.

use crate::app::Shell;
use crate::paint::{paint, place};
use gpui::{
    div, prelude::*, px, Context, Div, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent,
    Window,
};
use mapping::{MapField, MappingConfirm};
use om_ui::geometry::Rect;
use om_ui::mapping_editor::MappingEditorPage;
use om_ui::preferences::Preferences;
use om_ui::settings::{
    stats_text, AppearanceSettingsPage, AudioSettingsModel, AudioSettingsPage, AudioStats,
    AutomixControl, LibraryControl, LibrarySettingsPage, MidiInputs, MidiPage, MidiSettingsPage,
    PageKind, PluginHost, PluginsSettingsPage, RecordHost, SettingsError, SettingsPanel,
    SettingsResult,
};
use om_ui::stems::StemHost;
use om_ui::theme::{Source, ThemeRegistry, ThemeTokens};
use std::path::PathBuf;
use std::sync::Arc;

mod appearance;
mod audio;
mod automix;
pub(crate) mod chips;
mod library;
mod mapping;
mod midi;
mod overlay;
mod plugins;
mod recording;
mod stems;

/// The audio readout, read on the UI thread each frame.
pub type AudioStatsFn = Arc<dyn Fn() -> AudioStats + Send + Sync>;

/// Width and height the card wants; it shrinks to fit the window.
const CARD_WIDTH: i32 = 720;
const CARD_HEIGHT: i32 = 540;
const CARD_MARGIN: i32 = 24;

/// Copies the plugin themes into a window's theme registry.
pub type ThemeSync = Box<dyn Fn(&mut ThemeRegistry)>;

/// What the binary hands the window to run the settings.
pub struct SettingsOptions {
    pub prefs: Preferences,
    pub themes: ThemeRegistry,
    /// `None` leaves the Audio tab out (the panel does the same without a model).
    pub audio: Option<Box<dyn AudioSettingsModel>>,
    pub audio_stats: Option<AudioStatsFn>,
    /// Called after every edit that changed something: save and apply the preferences.
    pub on_changed: Box<dyn FnMut(&Preferences)>,
    /// The running MIDI service. `None` when MIDI could not start; the page says so.
    pub midi: Option<Arc<dyn MidiPage>>,
    /// The parameter ids MIDI learn offers, in the order the page lists them.
    pub learn_targets: Vec<String>,
    /// The library. `None` when it did not open: the watch-folder and import actions say so.
    pub library: Option<Box<dyn LibraryControl>>,
    /// The stem model and cache, behind the stems section of the Library page. `None` when the
    /// library did not open: the section says so.
    pub stems: Option<Box<dyn StemHost>>,
    /// The plugins, behind the Plugins page. `None` leaves the page out of use.
    pub plugins: Option<Box<dyn PluginHost>>,
    /// The automix service, behind the Automix page. `None` leaves the page without a service: its
    /// settings are still stored and the page says automix is idle.
    pub automix: Option<Box<dyn AutomixControl>>,
    /// The mapping editor, behind the Mapping tab. `None` when MIDI is not attached.
    pub mapping: Option<MappingEditorPage>,
    /// The recorder, behind the Recording tab. `None` leaves the tab storing settings only.
    pub record: Option<Box<dyn RecordHost>>,
    /// Makes this window's theme registry hold the plugins' themes. Called on start-up, when the
    /// overlay opens and after a plugin or preference change.
    pub sync_themes: Option<ThemeSync>,
    /// Opens the overlay at start-up on the tab with this title (`--settings=midi`), ignoring
    /// case. An unknown title opens the first tab.
    pub start_tab: Option<String>,
}

/// What a text field is for: what Enter does with the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// The folder recordings are written to.
    RecordFolder,
    /// A folder to add to the watch folders.
    WatchFolder,
    /// The mapping table filter text (applied as it is typed).
    MapFilter,
    /// A controller map (JSON) to import into the mapping editor.
    MapImport,
    /// Where the mapping editor exports its map.
    MapExport,
    /// One field of the mapping edit form.
    MapField(MapField),
    /// An iTunes or Rekordbox XML file to import.
    ImportXml,
    /// A controller map (JSON) to load into the MIDI service.
    MapFile,
    /// The stem cache size in whole GB (1 to 1000).
    MaxCacheGb,
}

/// The text field open at the foot of the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntry {
    pub kind: EntryKind,
    pub text: String,
}

/// Lists the pages show, read from the services when the overlay opens or a page acts, not each
/// frame (the MIDI backend and the library database are not free to ask).
#[derive(Default)]
pub struct PageLists {
    pub midi_inputs: Vec<String>,
    pub folders: Vec<PathBuf>,
    pub loaded_plugins: Vec<String>,
    /// Built-in plugins that are not loaded, as `(id, name)`.
    pub builtins: Vec<(String, String)>,
    /// Playlists the automix queue can play, as `(id, label)`.
    pub playlists: Vec<(i64, String)>,
}

/// What the last action on a page did, shown under the tabs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// The overlay's state. Edits go to `prefs` and the services, then `on_changed` is called.
pub struct SettingsState {
    pub panel: SettingsPanel,
    pub open: bool,
    pub prefs: Preferences,
    pub themes: ThemeRegistry,
    pub lists: PageLists,
    /// The text field, while a path is typed.
    pub entry: Option<TextEntry>,
    pub notice: Notice,
    /// Set when the overlay opens, so its card takes the keys on the next frame.
    pub(crate) focus_pending: bool,
    audio: Option<Box<dyn AudioSettingsModel>>,
    audio_stats: Option<AudioStatsFn>,
    on_changed: Box<dyn FnMut(&Preferences)>,
    /// Text of the last failed audio change, shown in place of the readout.
    setup_error: String,
    midi: Option<Arc<dyn MidiPage>>,
    /// When the MIDI input list was last read while the page was showing (see `poll_midi_inputs`).
    midi_polled: Option<std::time::Instant>,
    learn_targets: Vec<String>,
    /// The learn in progress: its target and the map generation when it started.
    learning: Option<(String, u64)>,
    library: Option<Box<dyn LibraryControl>>,
    stems: Option<Box<dyn StemHost>>,
    /// Bytes the stem cache holds, read when the overlay opens and after a stems action (the
    /// cache is not free to measure each frame).
    pub stems_bytes: u64,
    /// The Clear-cache confirmation is showing.
    pub stems_confirm_clear: bool,
    plugins: Option<Box<dyn PluginHost>>,
    automix: Option<Box<dyn AutomixControl>>,
    record: Option<Box<dyn RecordHost>>,
    sync_themes: Option<ThemeSync>,
    /// The mapping editor, when MIDI is attached.
    pub mapping: Option<MappingEditorPage>,
    /// The confirm step the Mapping tab is showing (Reset, or leaving with unsaved edits).
    pub mapping_confirm: Option<MappingConfirm>,
}

impl SettingsState {
    /// The audio readout now, when the window was given one.
    pub(crate) fn audio_snapshot(&self) -> Option<AudioStats> {
        self.audio_stats.as_ref().map(|read| read())
    }

    pub fn new(options: SettingsOptions) -> Self {
        let has_audio = options.audio.is_some();
        let mut state = Self {
            panel: SettingsPanel::new(has_audio),
            open: false,
            prefs: options.prefs,
            themes: options.themes,
            lists: PageLists::default(),
            entry: None,
            notice: Notice::default(),
            focus_pending: false,
            audio: options.audio,
            audio_stats: options.audio_stats,
            on_changed: options.on_changed,
            setup_error: String::new(),
            midi: options.midi,
            midi_polled: None,
            learn_targets: options.learn_targets,
            learning: None,
            library: options.library,
            stems: options.stems,
            stems_bytes: 0,
            stems_confirm_clear: false,
            plugins: options.plugins,
            automix: options.automix,
            record: options.record,
            mapping: options.mapping,
            mapping_confirm: None,
            sync_themes: options.sync_themes,
        };
        if let Some(title) = options.start_tab.as_deref() {
            let index = state
                .panel
                .tabs()
                .iter()
                .position(|t| t.eq_ignore_ascii_case(title))
                .unwrap_or(0);
            state.panel.show(index);
            state.open = true;
            state.focus_pending = true;
        }
        state.sync_plugin_themes();
        state.refresh_lists();
        state
    }

    /// Saves and applies the preferences, then makes the plugin themes current.
    fn changed(&mut self) {
        (self.on_changed)(&self.prefs);
        self.sync_plugin_themes();
    }

    /// Puts the registry's plugin themes into this window's registry.
    fn sync_plugin_themes(&mut self) {
        if let Some(sync) = &self.sync_themes {
            sync(&mut self.themes);
        }
    }

    /// Reads the device names, the watch folders and the plugin lists from the services.
    pub fn refresh_lists(&mut self) {
        if let Some(midi) = &self.midi {
            self.lists.midi_inputs = MidiInputs::input_names(&**midi);
        }
        if let Some(read) = self.library.as_ref().map(|library| library.folders()) {
            match read {
                Ok(folders) => self.lists.folders = folders,
                Err(error) => {
                    self.lists.folders.clear();
                    self.set_notice(format!("Could not read watch folders: {error}"), true);
                }
            }
        }
        if let Some(stems) = &self.stems {
            self.stems_bytes = stems.cache_bytes();
        }
        if let Some(plugins) = &self.plugins {
            self.lists.loaded_plugins = plugins.loaded_plugins();
            self.lists.builtins = plugins.builtins();
        }
        if let Some(automix) = &self.automix {
            match automix.playlists() {
                Ok(playlists) => self.lists.playlists = playlists,
                Err(error) => {
                    self.lists.playlists.clear();
                    self.notice = Notice {
                        text: format!("Could not read playlists: {error}"),
                        error: true,
                    };
                }
            }
        }
    }

    fn set_notice(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Notice {
            text: text.into(),
            error,
        };
    }

    // MIDI page.
}

#[cfg(test)]
mod fixtures {
    use super::*;
    use om_ui::preferences::Preferences;
    use om_ui::theme::ThemeRegistry;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;

    pub(super) fn state(
        audio: Option<Box<dyn AudioSettingsModel>>,
        changes: Rc<RefCell<usize>>,
    ) -> SettingsState {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        SettingsState::new(SettingsOptions {
            prefs: Preferences::default(),
            themes,
            audio,
            audio_stats: None,
            on_changed: Box::new(move |_| *changes.borrow_mut() += 1),
            midi: None,
            learn_targets: Vec::new(),
            library: None,
            stems: None,
            plugins: None,
            automix: None,
            record: None,
            mapping: None,
            sync_themes: None,
            start_tab: None,
        })
    }

    pub(super) fn options(
        midi: Option<Arc<dyn MidiPage>>,
        library: Option<Box<dyn LibraryControl>>,
        plugins: Option<Box<dyn PluginHost>>,
        syncs: Rc<Cell<usize>>,
        changes: Rc<Cell<usize>>,
    ) -> SettingsOptions {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        SettingsOptions {
            prefs: Preferences::default(),
            themes,
            audio: None,
            audio_stats: None,
            on_changed: Box::new(move |_| changes.set(changes.get() + 1)),
            midi,
            learn_targets: vec!["deck.0.play".to_owned()],
            library,
            stems: None,
            plugins,
            automix: None,
            record: None,
            mapping: None,
            sync_themes: Some(Box::new(move |_| syncs.set(syncs.get() + 1))),
            start_tab: None,
        }
    }
}
