//! The settings overlay (Todo 29): a card with the five tabs over the window, opened by the Settings
//! button in the header and closed by its Close button, Escape or a click on the scrim. Port of the
//! drawing in `src/ui/SettingsPanel.cpp` and the pages. The logic (what each control does, what is
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
use om_ui::geometry::Rect;
use om_ui::preferences::Preferences;
use om_ui::settings::{
    stats_text, AppearanceSettingsPage, AudioSettingsModel, AudioSettingsPage, AudioStats,
    LibraryControl, LibrarySettingsPage, MidiInputs, MidiPage, MidiSettingsPage, PageKind,
    PluginHost, PluginsSettingsPage, SettingsPanel, SettingsResult,
};
use om_ui::theme::{Source, ThemeRegistry, ThemeTokens};
use std::path::PathBuf;
use std::sync::Arc;

/// The audio readout, read on the UI thread each frame.
pub type AudioStatsFn = Arc<dyn Fn() -> AudioStats + Send + Sync>;

/// Width and height the card wants; it shrinks to fit the window.
const CARD_WIDTH: i32 = 720;
const CARD_HEIGHT: i32 = 540;
const CARD_MARGIN: i32 = 24;
/// Height of the header strip above the layout, with the Settings button.
pub const HEADER_HEIGHT: i32 = 32;
/// Minutes the Library page offers for the rescan timer.
const RESCAN_CHOICES: [i32; 6] = [1, 5, 15, 30, 60, 240];

/// What the binary hands the window to run the settings.
pub struct SettingsOptions {
    pub prefs: Preferences,
    pub themes: ThemeRegistry,
    /// `None` leaves the Audio tab out (the C++ panel does the same without a model).
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
    /// The plugins, behind the Plugins page. `None` leaves the page out of use.
    pub plugins: Option<Box<dyn PluginHost>>,
    /// Makes this window's theme registry hold the plugins' themes. Called on start-up, when the
    /// overlay opens and after a plugin or preference change.
    pub sync_themes: Option<Box<dyn Fn(&mut ThemeRegistry)>>,
}

/// What a text field is for: what Enter does with the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A folder to add to the watch folders.
    WatchFolder,
    /// An iTunes or Rekordbox XML file to import.
    ImportXml,
    /// A controller map (JSON) to load into the MIDI service.
    MapFile,
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
    learn_targets: Vec<String>,
    /// The learn in progress: its target and the map generation when it started.
    learning: Option<(String, u64)>,
    library: Option<Box<dyn LibraryControl>>,
    plugins: Option<Box<dyn PluginHost>>,
    sync_themes: Option<Box<dyn Fn(&mut ThemeRegistry)>>,
}

/// Stands in for the library when it did not open: the scan settings are still stored, and the
/// library actions report that there is no library.
struct NoLibrary;

const NO_LIBRARY: &str = "The library is not available";

impl LibraryControl for NoLibrary {
    fn available(&self) -> bool {
        false
    }

    fn add_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
        Err(NO_LIBRARY.to_owned())
    }

    fn remove_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
        Err(NO_LIBRARY.to_owned())
    }

    fn rescan(&mut self) -> SettingsResult<()> {
        Err(NO_LIBRARY.to_owned())
    }
}

/// The device names the page was last given, as the MIDI page's input list.
struct Listed<'a>(&'a [String]);

impl MidiInputs for Listed<'_> {
    fn input_names(&self) -> Vec<String> {
        self.0.to_vec()
    }
}

/// Plain values the Audio page shows, read once per frame.
struct AudioView {
    types: Vec<String>,
    device_type: String,
    devices: Vec<String>,
    device: String,
    rates: Vec<u32>,
    rate: u32,
    buffers: Vec<u32>,
    buffer: u32,
    pairs: Vec<String>,
    master: usize,
    cue: Option<usize>,
    readout: String,
}

impl SettingsState {
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
            learn_targets: options.learn_targets,
            learning: None,
            library: options.library,
            plugins: options.plugins,
            sync_themes: options.sync_themes,
        };
        state.sync_plugin_themes();
        state.refresh_lists();
        state
    }

    /// Runs one audio edit. A failure becomes the readout's text; a change is saved and applied.
    pub fn edit_audio(&mut self, edit: impl FnOnce(&mut AudioSettingsPage<'_>) -> SettingsResult) {
        let Some(model) = self.audio.as_deref_mut() else {
            return;
        };
        let result = {
            let mut page = AudioSettingsPage::new(&mut self.prefs, model);
            edit(&mut page)
        };
        match result {
            Ok(changed) => {
                self.setup_error.clear();
                if changed {
                    self.changed();
                }
            }
            Err(text) => self.setup_error = text,
        }
    }

    /// Stores the theme and selects it in this window's registry (the live switch). `false` when
    /// the id is unknown, in which case nothing changes.
    pub fn pick_theme(&mut self, id: &str) -> bool {
        let result = {
            let mut page = AppearanceSettingsPage::new(&mut self.prefs, &self.themes);
            page.pick(id)
        };
        match result {
            Ok(changed) => {
                self.themes.select(id);
                if changed {
                    self.changed();
                }
                true
            }
            Err(_) => false,
        }
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
        if let Some(library) = &self.library {
            self.lists.folders = library.folders();
        }
        if let Some(plugins) = &self.plugins {
            self.lists.loaded_plugins = plugins.loaded_plugins();
            self.lists.builtins = plugins.builtins();
        }
    }

    pub fn midi_available(&self) -> bool {
        self.midi.is_some()
    }

    pub fn library_available(&self) -> bool {
        self.library.is_some()
    }

    pub fn learn_targets(&self) -> &[String] {
        &self.learn_targets
    }

    /// The target MIDI learn is waiting on, if one is armed.
    pub fn learning_target(&self) -> Option<&str> {
        self.learning.as_ref().map(|(target, _)| target.as_str())
    }

    fn set_notice(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Notice {
            text: text.into(),
            error,
        };
    }

    // MIDI page.

    /// Stores the "all inputs" switch.
    pub fn midi_set_enable_all(&mut self, all: bool) {
        let changed = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .set_enable_all(all);
        if changed {
            self.changed();
        }
    }

    /// Adds or removes one input from the explicit selection.
    pub fn midi_toggle_input(&mut self, name: &str) {
        let on = !self.prefs.midi.enabled_inputs.iter().any(|n| n == name);
        let changed = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .set_input_enabled(name, on);
        if changed {
            self.changed();
        }
    }

    /// Arms MIDI learn for `target`. The status says so until a control is learned or the learn is
    /// cancelled.
    pub fn midi_learn(&mut self, target: &str) {
        let Some(midi) = self.midi.clone() else {
            return self.set_notice("MIDI is not available", true);
        };
        let generation = midi.map_generation();
        let inputs = Listed(&self.lists.midi_inputs);
        let page = MidiSettingsPage::new(&mut self.prefs, &inputs);
        page.learn(midi.as_ref(), target);
        self.learning = Some((target.to_owned(), generation));
        self.set_notice(
            format!("Learning {target}: move a control on the controller. Cancel stops it."),
            false,
        );
    }

    pub fn midi_cancel_learn(&mut self) {
        let Some(midi) = self.midi.clone() else {
            return;
        };
        let inputs = Listed(&self.lists.midi_inputs);
        let page = MidiSettingsPage::new(&mut self.prefs, &inputs);
        page.cancel_learn(midi.as_ref());
        self.learning = None;
        self.set_notice("Learn cancelled", false);
    }

    /// Called each frame: when the MIDI service has stopped learning, says what happened.
    pub fn poll_learn(&mut self) {
        let Some((target, generation)) = self.learning.clone() else {
            return;
        };
        let Some(midi) = self.midi.clone() else {
            self.learning = None;
            return;
        };
        if midi.is_learning() {
            return;
        }
        self.learning = None;
        if midi.map_generation() == generation {
            self.set_notice(
                format!("Learn for {target} ended: no control was received"),
                true,
            );
        } else {
            self.set_notice(format!("Learned a control for {target}"), false);
        }
    }

    /// Loads `file` as the controller map. The preference is stored only when the file loads.
    pub fn midi_load_map(&mut self, file: PathBuf) {
        let Some(midi) = self.midi.clone() else {
            return self.set_notice("MIDI is not available", true);
        };
        match midi.load_map(&file) {
            Ok(()) => {
                let shown = file.display().to_string();
                let changed =
                    MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
                        .set_map_file(Some(file));
                self.set_notice(format!("Loaded the map {shown}"), false);
                if changed {
                    self.changed();
                }
            }
            Err(error) => self.set_notice(format!("Map not loaded: {error}"), true),
        }
    }

    /// Loads the map the preferences name (or the default one) into the running service.
    pub fn midi_load_current_map(&mut self) {
        let Some(midi) = self.midi.clone() else {
            return self.set_notice("MIDI is not available", true);
        };
        let shown = self.prefs.effective_map_file().display().to_string();
        let inputs = Listed(&self.lists.midi_inputs);
        let page = MidiSettingsPage::new(&mut self.prefs, &inputs);
        match page.load_map_now(midi.as_ref()) {
            Ok(()) => self.set_notice(format!("Loaded the map {shown}"), false),
            Err(error) => self.set_notice(format!("Map not loaded: {error}"), true),
        }
    }

    /// Back to the default map file.
    pub fn midi_use_default_map(&mut self) {
        let changed = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .set_map_file(None);
        if changed {
            self.changed();
        }
        self.set_notice("The default map file is used from the next load", false);
    }

    // Library page.

    /// Runs `f` on the Library page over the library, or over the stand-in when there is none, so
    /// the scan settings are stored either way.
    fn with_library_page<T>(
        &mut self,
        f: impl FnOnce(&mut LibrarySettingsPage<'_>) -> SettingsResult<T>,
    ) -> SettingsResult<T> {
        let mut none = NoLibrary;
        let library: &mut dyn LibraryControl = match self.library.as_deref_mut() {
            Some(library) => library,
            None => &mut none,
        };
        let mut page = LibrarySettingsPage::new(&mut self.prefs, library);
        f(&mut page)
    }

    pub fn library_set_scan_on_start(&mut self, on: bool) {
        if matches!(
            self.with_library_page(|p| Ok(p.set_scan_on_start(on))),
            Ok(true)
        ) {
            self.changed();
        }
    }

    pub fn library_set_interval(&mut self, minutes: i32) {
        if matches!(
            self.with_library_page(|p| Ok(p.set_scan_interval(minutes))),
            Ok(true)
        ) {
            self.changed();
        }
    }

    pub fn library_add_folder(&mut self, folder: PathBuf) {
        let shown = folder.display().to_string();
        match self.with_library_page(|p| p.add_watch_folder(folder)) {
            Ok(()) => {
                self.refresh_lists();
                self.set_notice(format!("Watching {shown}"), false);
            }
            Err(error) => self.set_notice(error, true),
        }
    }

    pub fn library_remove_folder(&mut self, folder: PathBuf) {
        let shown = folder.display().to_string();
        match self.with_library_page(|p| p.remove_watch_folder(folder)) {
            Ok(()) => {
                self.refresh_lists();
                self.set_notice(format!("Stopped watching {shown}"), false);
            }
            Err(error) => self.set_notice(error, true),
        }
    }

    pub fn library_rescan(&mut self) {
        match self.with_library_page(|p| p.rescan()) {
            Ok(()) => self.set_notice("Rescan started", false),
            Err(error) => self.set_notice(error, true),
        }
    }

    pub fn library_import(&mut self, xml: PathBuf) {
        match self.with_library_page(|p| p.import_xml(xml)) {
            Ok(summary) => self.set_notice(summary, false),
            Err(error) => self.set_notice(format!("Import failed: {error}"), true),
        }
    }

    // Plugins page.

    /// Runs `f` on the Plugins page over the plugin host. Without a host the page says so.
    fn with_plugins_page<T>(
        &mut self,
        f: impl FnOnce(&mut PluginsSettingsPage<'_>) -> SettingsResult<T>,
    ) -> SettingsResult<T> {
        let Some(host) = self.plugins.as_deref_mut() else {
            return Err("Plugins are not available".to_owned());
        };
        let mut page = PluginsSettingsPage::new(&self.prefs, host);
        f(&mut page)
    }

    /// After a plugin change: the registry's plugin themes follow, the lists are read again, and
    /// the page says what happened.
    fn plugin_changed(&mut self, result: SettingsResult<()>, done: String) {
        match result {
            Ok(()) => {
                self.sync_plugin_themes();
                self.refresh_lists();
                self.set_notice(done, false);
            }
            Err(error) => self.set_notice(error, true),
        }
    }

    pub fn plugin_load_builtin(&mut self, id: &str) {
        let result = self.with_plugins_page(|p| p.load_builtin(id));
        self.plugin_changed(result, format!("Loaded {id}"));
    }

    pub fn plugin_unload(&mut self, id: &str) {
        let result = self.with_plugins_page(|p| p.unload(id));
        self.plugin_changed(result, format!("Unloaded {id}"));
    }

    pub fn plugin_reload(&mut self) {
        let result = self.with_plugins_page(|p| p.reload());
        self.plugin_changed(result, "Reloaded the plugin folder".to_owned());
    }

    /// How many themes the plugins provide (they appear in the Appearance page).
    fn plugin_theme_count(&self) -> usize {
        self.themes
            .themes()
            .iter()
            .filter(|e| e.source == Source::Plugin)
            .count()
    }

    // Text field.

    pub fn begin_entry(&mut self, kind: EntryKind) {
        self.entry = Some(TextEntry {
            kind,
            text: String::new(),
        });
    }

    pub fn cancel_entry(&mut self) {
        self.entry = None;
    }

    pub fn type_text(&mut self, text: &str) {
        if let Some(entry) = self.entry.as_mut() {
            entry.text.push_str(text);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(entry) = self.entry.as_mut() {
            entry.text.pop();
        }
    }

    /// Enter: does what the field is for with the typed path. An empty path keeps the field open.
    pub fn submit_entry(&mut self) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        let text = entry.text.trim().to_owned();
        if text.is_empty() {
            self.entry = Some(entry);
            self.set_notice("Type a path first", true);
            return;
        }
        let path = PathBuf::from(text);
        match entry.kind {
            EntryKind::WatchFolder => self.library_add_folder(path),
            EntryKind::ImportXml => self.library_import(path),
            EntryKind::MapFile => self.midi_load_map(path),
        }
    }

    /// The Audio page's values, read through the page logic.
    fn audio_view(&mut self) -> Option<AudioView> {
        let stats = self.audio_stats.as_ref().map(|f| f()).unwrap_or_default();
        let readout = stats_text(&stats, &self.setup_error);
        let model = self.audio.as_deref_mut()?;
        let types = model.device_types();
        let device_type = model.device_type();
        let devices = model.output_devices();
        let device = model.output_device();
        let rates = model.sample_rates();
        let rate = model.sample_rate();
        let buffers = model.buffer_sizes();
        let buffer = model.buffer_size();
        let page = AudioSettingsPage::new(&mut self.prefs, model);
        Some(AudioView {
            types,
            device_type,
            devices,
            device,
            rates,
            rate,
            buffers,
            buffer,
            pairs: page.pair_labels(),
            master: page.audio_prefs().master_pair.max(0) as usize,
            cue: page.shown_cue_pair(),
            readout,
        })
    }
}

/// The accent for lit chips: the theme's first accent colour.
fn accent_of(t: &ThemeTokens) -> om_ui::theme::Colour {
    t.accents.first().copied().unwrap_or(t.surface_raised)
}

/// A flow-layout button: a labelled chip that runs `on_press` on the Shell when clicked.
fn chip(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(px(10.))
        .py(px(4.))
        .mr(px(6.))
        .mb(px(6.))
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(if lit { accent_of(t) } else { t.surface_raised }))
        .text_color(paint(if lit { t.on_accent } else { t.text_primary }))
        .text_size(px(t.type_small))
        .child(label.to_owned())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                on_press(this);
                cx.notify();
            }),
        )
}

/// A labelled row of chips.
fn choice_row(t: &ThemeTokens, title: &str, chips: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .mb(px(10.))
        .child(
            div()
                .text_color(paint(t.text_secondary))
                .text_size(px(t.type_caption))
                .mb(px(4.))
                .child(title.to_owned()),
        )
        .child(div().flex().flex_wrap().child(chips))
}

fn note(t: &ThemeTokens, text: &str) -> Div {
    div()
        .text_color(paint(t.text_secondary))
        .text_size(px(t.type_caption))
        .mb(px(8.))
        .child(text.to_owned())
}

/// The last action's text, in the error colour when it failed.
fn status_line(t: &ThemeTokens, notice: &Notice) -> Div {
    if notice.text.is_empty() {
        return div();
    }
    let colour = if notice.error {
        t.text_primary
    } else {
        t.text_secondary
    };
    let text = if notice.error {
        format!("Error: {}", notice.text)
    } else {
        notice.text.clone()
    };
    div()
        .text_color(paint(colour))
        .text_size(px(t.type_caption))
        .mt(px(6.))
        .child(text)
}

/// A row of chips.
fn row_of(chips: Vec<Div>) -> Div {
    chips
        .into_iter()
        .fold(div().flex().flex_wrap(), |row, c| row.child(c))
}

/// The text field, when the open field is `kind`: the typed path with Apply and Cancel.
fn entry_field(
    t: &ThemeTokens,
    settings: &SettingsState,
    kind: EntryKind,
    hint: &str,
    cx: &mut Context<Shell>,
) -> Option<Div> {
    let entry = settings.entry.as_ref().filter(|e| e.kind == kind)?;
    let field = div()
        .flex_1()
        .overflow_hidden()
        .px(px(8.))
        .py(px(4.))
        .mr(px(6.))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
        .child(format!("{}|", entry.text));
    let line = div()
        .flex()
        .items_center()
        .mb(px(6.))
        .child(field)
        .child(chip(t, "Apply", false, cx, |this| {
            this.settings_mut(|s| s.submit_entry())
        }))
        .child(chip(t, "Cancel", false, cx, |this| {
            this.settings_mut(|s| s.cancel_entry())
        }));
    Some(
        div()
            .flex()
            .flex_col()
            .child(note(
                t,
                &format!("Type the {hint}, then Enter. Escape cancels."),
            ))
            .child(line),
    )
}

impl Shell {
    /// Runs `f` on the settings when the window has them.
    fn settings_mut(&mut self, f: impl FnOnce(&mut SettingsState)) {
        if let Some(settings) = self.settings.as_mut() {
            f(settings);
        }
    }

    /// Takes the window's theme from the settings registry (after a theme or plugin change).
    fn refresh_theme(&mut self) {
        if let Some(settings) = self.settings.as_ref() {
            self.theme = settings.themes.current().clone();
        }
    }

    /// The header strip with the Settings button, when the window has settings.
    pub(crate) fn header(&self, y: i32, width: i32, cx: &mut Context<Shell>) -> Option<Div> {
        self.settings.as_ref()?;
        let t = self.theme.clone();
        let button = chip(&t, "Settings", false, cx, |this| this.open_settings());
        Some(
            place(Rect::new(0, y, width, HEADER_HEIGHT))
                .bg(paint(t.popup))
                .flex()
                .justify_end()
                .items_center()
                .px(px(8.))
                .child(button),
        )
    }

    pub(crate) fn open_settings(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.refresh_lists();
            settings.open = true;
            settings.focus_pending = true;
        }
    }

    pub(crate) fn close_settings(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.open = false;
            settings.entry = None;
        }
    }

    /// A key while the overlay is open: the text field takes the typed text (Enter applies,
    /// Escape cancels it); otherwise Escape closes the overlay.
    pub(crate) fn settings_key(&mut self, key: &str, typed: Option<&str>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if !settings.open {
            return;
        }
        if settings.entry.is_some() {
            match key {
                "enter" => settings.submit_entry(),
                "escape" => settings.cancel_entry(),
                "backspace" => settings.backspace(),
                _ => {
                    if let Some(text) = typed {
                        settings.type_text(text);
                    }
                }
            }
        } else if key == "escape" {
            self.close_settings();
        }
    }

    /// The overlay for a window of `area` size, or `None` while it is closed.
    pub(crate) fn settings_overlay(
        &mut self,
        area: Rect,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Option<Div> {
        let t = self.theme.clone();
        let focus: FocusHandle = self
            .settings_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let settings = self.settings.as_mut()?;
        if !settings.open {
            return None;
        }
        if std::mem::take(&mut settings.focus_pending) {
            focus.focus(window);
        }
        settings.poll_learn();
        let w = CARD_WIDTH.min(area.w - 2 * CARD_MARGIN).max(0);
        let h = CARD_HEIGHT.min(area.h - 2 * CARD_MARGIN).max(0);
        let card_x = area.x + (area.w - w) / 2;
        let card_y = area.y + (area.h - h) / 2;

        // Tabs, then the page of the tab shown.
        let current = settings.panel.current();
        let mut tabs = div().flex().flex_wrap().mb(px(10.));
        for (index, title) in settings.panel.tabs().to_vec().into_iter().enumerate() {
            tabs = tabs.child(chip(&t, &title, index == current, cx, move |this| {
                this.settings_mut(|s| {
                    s.panel.show(index);
                    s.refresh_lists();
                });
            }));
        }
        let body = match settings.panel.page_kind(current) {
            Some(PageKind::Audio) => match settings.audio_view() {
                Some(view) => audio_page(&t, &view, cx),
                None => note(&t, "No audio device model."),
            },
            Some(PageKind::Appearance) => appearance_page(&t, settings, cx),
            Some(PageKind::Midi) => midi_page(&t, settings, cx),
            Some(PageKind::Library) => library_page(&t, settings, cx),
            Some(PageKind::Plugins) => plugins_page(&t, settings, cx),
            None => note(&t, "This page is added by a plugin."),
        };
        let status = status_line(&t, &settings.notice);
        let close = chip(&t, "Close", false, cx, |this| this.close_settings());

        let click_focus = focus.clone();
        let card = place(Rect::new(card_x, card_y, w, h))
            .flex()
            .flex_col()
            .p(px(14.))
            .bg(paint(t.popup))
            .border_1()
            .border_color(paint(t.border))
            .rounded(px(t.control_radius))
            .text_color(paint(t.text_primary))
            .text_size(px(t.type_small))
            .overflow_hidden()
            .track_focus(&focus)
            .child(
                div()
                    .text_size(px(t.type_heading))
                    .mb(px(8.))
                    .child("Settings"),
            )
            .child(tabs)
            .child(div().flex_1().overflow_hidden().child(body))
            .child(status)
            .child(div().flex().justify_end().child(close))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                let keys = &ev.keystroke;
                if keys.modifiers.control || keys.modifiers.platform {
                    return;
                }
                this.settings_key(&keys.key, keys.key_char.as_deref());
                cx.notify();
            }))
            // Clicks on the card do not reach the scrim below, and the card takes the keys.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, window, cx| {
                    click_focus.focus(window);
                    cx.stop_propagation();
                }),
            );

        let scrim = place(area)
            .bg(paint(t.backdrop_top))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.close_settings();
                    cx.notify();
                }),
            )
            .child(card);
        Some(scrim)
    }
}

fn audio_page(t: &ThemeTokens, view: &AudioView, cx: &mut Context<Shell>) -> Div {
    let mut types = Vec::new();
    for name in &view.types {
        let (lit, name) = (*name == view.device_type, name.clone());
        types.push(chip(t, &name.clone(), lit, cx, move |this| {
            let name = name.clone();
            this.settings_mut(|s| s.edit_audio(|page| page.set_device_type(&name)));
        }));
    }
    let mut devices = Vec::new();
    for name in &view.devices {
        let (lit, name) = (*name == view.device, name.clone());
        devices.push(chip(t, &name.clone(), lit, cx, move |this| {
            let name = name.clone();
            this.settings_mut(|s| s.edit_audio(|page| page.set_output_device(&name)));
        }));
    }
    let mut rates = Vec::new();
    for rate in &view.rates {
        let rate = *rate;
        rates.push(chip(
            t,
            &format!("{rate} Hz"),
            rate == view.rate,
            cx,
            move |this| {
                this.settings_mut(|s| s.edit_audio(|page| page.set_sample_rate(rate)));
            },
        ));
    }
    let mut buffers = Vec::new();
    for size in &view.buffers {
        let size = *size;
        buffers.push(chip(
            t,
            &format!("{size} samples"),
            size == view.buffer,
            cx,
            move |this| {
                this.settings_mut(|s| s.edit_audio(|page| page.set_buffer_size(size)));
            },
        ));
    }
    let mut master = Vec::new();
    for (index, label) in view.pairs.iter().enumerate() {
        master.push(chip(t, label, index == view.master, cx, move |this| {
            this.settings_mut(|s| s.edit_audio(|page| page.set_master_pair(index)));
        }));
    }
    let mut cue = vec![chip(t, "None", view.cue.is_none(), cx, |this| {
        this.settings_mut(|s| s.edit_audio(|page| page.set_cue_pair(None)));
    })];
    for (index, label) in view.pairs.iter().enumerate() {
        cue.push(chip(t, label, view.cue == Some(index), cx, move |this| {
            this.settings_mut(|s| s.edit_audio(|page| page.set_cue_pair(Some(index))));
        }));
    }

    let mut column = div().flex().flex_col();
    column = column.child(choice_row(t, "Driver", row_of(types)));
    column = column.child(choice_row(t, "Output device", row_of(devices)));
    column = column.child(choice_row(t, "Sample rate", row_of(rates)));
    column = column.child(choice_row(t, "Buffer size", row_of(buffers)));
    column = column.child(choice_row(t, "Master output", row_of(master)));
    column = column.child(choice_row(t, "Headphone output", row_of(cue)));
    column = column.child(note(
        t,
        "Master and headphone mixes go to the chosen stereo pairs of the output device.",
    ));
    column.child(
        div()
            .text_color(paint(t.text_secondary))
            .text_size(px(t.type_caption))
            .child(view.readout.clone()),
    )
}

fn appearance_page(t: &ThemeTokens, settings: &mut SettingsState, cx: &mut Context<Shell>) -> Div {
    let choices = AppearanceSettingsPage::new(&mut settings.prefs, &settings.themes).choices();
    let stored = settings.prefs.theme.clone();
    let mut chips = Vec::new();
    for (id, name) in choices {
        let lit = id == stored;
        chips.push(chip(t, &name, lit, cx, move |this| {
            let id = id.clone();
            this.settings_mut(|s| {
                s.pick_theme(&id);
            });
            // The live switch: the window takes the new theme on the next frame.
            this.refresh_theme();
        }));
    }
    div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Theme", row_of(chips)))
        .child(note(
            t,
            "The theme applies at once and is saved with the other preferences. Plugin themes are listed here while their plugin is loaded.",
        ))
}

fn midi_page(t: &ThemeTokens, settings: &SettingsState, cx: &mut Context<Shell>) -> Div {
    if !settings.midi_available() {
        return note(
            t,
            "MIDI is not available (see the terminal for the reason).",
        );
    }
    let all = settings.prefs.midi.enable_all;
    let mut inputs = vec![chip(t, "All inputs", all, cx, move |this| {
        this.settings_mut(|s| s.midi_set_enable_all(!all))
    })];
    for name in &settings.lists.midi_inputs {
        let lit = all || settings.prefs.midi.enabled_inputs.contains(name);
        let press = name.clone();
        inputs.push(chip(t, name, lit, cx, move |this| {
            let name = press.clone();
            this.settings_mut(|s| s.midi_toggle_input(&name))
        }));
    }
    let mut inputs_column = div().flex().flex_col().child(choice_row(
        t,
        "Input devices (turn off All inputs to choose)",
        row_of(inputs),
    ));
    if settings.lists.midi_inputs.is_empty() {
        inputs_column = inputs_column.child(note(t, "No MIDI input device is listed."));
    }

    let map_row = row_of(vec![
        chip(t, "Load map file…", false, cx, |this| {
            this.settings_mut(|s| s.begin_entry(EntryKind::MapFile))
        }),
        chip(t, "Load the current map", false, cx, |this| {
            this.settings_mut(|s| s.midi_load_current_map())
        }),
        chip(t, "Use the default map", false, cx, |this| {
            this.settings_mut(|s| s.midi_use_default_map())
        }),
    ]);
    let map_in_use = settings.prefs.effective_map_file();
    let mut column = inputs_column
        .child(note(t, &format!("Map file: {}", map_in_use.display())))
        .child(choice_row(t, "Controller map", map_row));
    if let Some(field) = entry_field(
        t,
        settings,
        EntryKind::MapFile,
        "controller map (JSON) path",
        cx,
    ) {
        column = column.child(field);
    }

    let mut learn: Vec<Div> = Vec::new();
    for target in settings.learn_targets() {
        let lit = settings.learning_target() == Some(target.as_str());
        let press = target.clone();
        learn.push(chip(t, target, lit, cx, move |this| {
            let target = press.clone();
            this.settings_mut(|s| s.midi_learn(&target))
        }));
    }
    if settings.learning_target().is_some() {
        learn.push(chip(t, "Cancel learn", true, cx, |this| {
            this.settings_mut(|s| s.midi_cancel_learn())
        }));
    }
    column.child(choice_row(
        t,
        "MIDI learn: pick a target, then move a control",
        row_of(learn),
    ))
}

fn library_page(t: &ThemeTokens, settings: &SettingsState, cx: &mut Context<Shell>) -> Div {
    let mut folders = div().flex().flex_col();
    if !settings.library_available() {
        folders = folders.child(note(
            t,
            "The library is not available (see the terminal for the reason).",
        ));
    } else if settings.lists.folders.is_empty() {
        folders = folders.child(note(
            t,
            "No watch folder yet. Add one to scan it into the library.",
        ));
    }
    for folder in &settings.lists.folders {
        let press = folder.clone();
        folders = folders.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_color(paint(t.text_primary))
                        .child(folder.display().to_string()),
                )
                .child(chip(t, "Remove", false, cx, move |this| {
                    let folder = press.clone();
                    this.settings_mut(|s| s.library_remove_folder(folder))
                })),
        );
    }

    let mut intervals = Vec::new();
    for minutes in RESCAN_CHOICES {
        let lit = settings.prefs.library.scan_interval_minutes == minutes;
        intervals.push(chip(t, &format!("{minutes} min"), lit, cx, move |this| {
            this.settings_mut(|s| s.library_set_interval(minutes))
        }));
    }
    let scan_on = settings.prefs.library.scan_on_start;
    let on_start = chip(
        t,
        if scan_on { "On" } else { "Off" },
        scan_on,
        cx,
        move |this| this.settings_mut(|s| s.library_set_scan_on_start(!scan_on)),
    );

    let mut column = div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Watch folders", folders))
        .child(choice_row(
            t,
            "Folders",
            row_of(vec![
                chip(t, "Add watch folder…", false, cx, |this| {
                    this.settings_mut(|s| s.begin_entry(EntryKind::WatchFolder))
                }),
                chip(t, "Rescan now", false, cx, |this| {
                    this.settings_mut(|s| s.library_rescan())
                }),
            ]),
        ));
    if let Some(field) = entry_field(t, settings, EntryKind::WatchFolder, "folder path", cx) {
        column = column.child(field);
    }
    column = column
        .child(choice_row(t, "Rescan every", row_of(intervals)))
        .child(choice_row(
            t,
            "Scan the folders on start",
            row_of(vec![on_start]),
        ));
    column = column.child(choice_row(
        t,
        "Import a library",
        row_of(vec![chip(
            t,
            "Import iTunes or Rekordbox XML…",
            false,
            cx,
            |this| this.settings_mut(|s| s.begin_entry(EntryKind::ImportXml)),
        )]),
    ));
    if let Some(field) = entry_field(t, settings, EntryKind::ImportXml, "XML file path", cx) {
        column = column.child(field);
    }
    column
}

fn plugins_page(t: &ThemeTokens, settings: &SettingsState, cx: &mut Context<Shell>) -> Div {
    let mut loaded = div().flex().flex_col();
    if settings.lists.loaded_plugins.is_empty() {
        loaded = loaded.child(note(t, "No plugin is loaded."));
    }
    for id in &settings.lists.loaded_plugins {
        let press = id.clone();
        loaded = loaded.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(div().flex_1().overflow_hidden().child(id.clone()))
                .child(chip(t, "Unload", false, cx, move |this| {
                    let id = press.clone();
                    this.settings_mut(|s| s.plugin_unload(&id));
                    this.refresh_theme();
                })),
        );
    }

    let mut builtins = div().flex().flex_col();
    if settings.lists.builtins.is_empty() {
        builtins = builtins.child(note(t, "No built-in plugin to load."));
    }
    for (id, name) in &settings.lists.builtins {
        let press = id.clone();
        builtins = builtins.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(format!("{name} ({id})")),
                )
                .child(chip(t, "Load", false, cx, move |this| {
                    let id = press.clone();
                    this.settings_mut(|s| s.plugin_load_builtin(&id));
                    this.refresh_theme();
                })),
        );
    }

    div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Loaded plugins", loaded))
        .child(choice_row(t, "Built-in plugins", builtins))
        .child(choice_row(
            t,
            "Folder",
            row_of(vec![chip(t, "Reload the plugin folder", false, cx, |this| {
                this.settings_mut(|s| s.plugin_reload());
                this.refresh_theme();
            })]),
        ))
        .child(note(
            t,
            &format!(
                "Plugin folder: {}. Shared-library plugins in it are reported, not loaded (dynamic loading is not decided). {} plugin theme(s) are in Appearance.",
                settings.prefs.effective_plugin_folder().display(),
                settings.plugin_theme_count()
            ),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::settings::SettingsResult;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Records calls; fails a rate of 22 050 Hz so the error path runs.
    #[derive(Default)]
    struct FakeAudio {
        rate: u32,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl AudioSettingsModel for FakeAudio {
        fn device_types(&self) -> Vec<String> {
            vec!["Fake".into()]
        }
        fn device_type(&self) -> String {
            "Fake".into()
        }
        fn set_device_type(&mut self, _: &str) -> SettingsResult<()> {
            Ok(())
        }
        fn output_devices(&self) -> Vec<String> {
            vec!["Out".into()]
        }
        fn output_device(&self) -> String {
            "Out".into()
        }
        fn set_output_device(&mut self, _: &str) -> SettingsResult<()> {
            Ok(())
        }
        fn sample_rates(&self) -> Vec<u32> {
            vec![44_100, 48_000]
        }
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn set_sample_rate(&mut self, rate: u32) -> SettingsResult<()> {
            if rate == 22_050 {
                return Err("unsupported device configuration: 22050 Hz".into());
            }
            self.rate = rate;
            self.log.borrow_mut().push(format!("rate {rate}"));
            Ok(())
        }
        fn buffer_sizes(&self) -> Vec<u32> {
            vec![256, 512]
        }
        fn buffer_size(&self) -> u32 {
            512
        }
        fn set_buffer_size(&mut self, _: u32) -> SettingsResult<()> {
            Ok(())
        }
        fn output_channels(&self) -> usize {
            2
        }
        fn state_string(&self) -> String {
            format!("rate={}", self.rate)
        }
    }

    fn state(
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
            plugins: None,
            sync_themes: None,
        })
    }

    #[test]
    fn an_audio_change_notifies_once_and_repeating_it_does_not() {
        let changes = Rc::new(RefCell::new(0));
        let log = Rc::new(RefCell::new(Vec::new()));
        let audio = FakeAudio {
            rate: 48_000,
            log: Rc::clone(&log),
        };
        let mut s = state(Some(Box::new(audio)), Rc::clone(&changes));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        assert_eq!(*changes.borrow(), 1);
        // The model is asked both times (as in C++); only the stored state decides the change.
        assert_eq!(
            *log.borrow(),
            vec!["rate 44100".to_owned(), "rate 44100".to_owned()]
        );
        assert_eq!(s.prefs.audio.device_state, "rate=44100");
    }

    #[test]
    fn a_failed_audio_change_is_kept_as_the_readout_text_and_notifies_nobody() {
        let changes = Rc::new(RefCell::new(0));
        let audio = FakeAudio {
            rate: 48_000,
            ..FakeAudio::default()
        };
        let mut s = state(Some(Box::new(audio)), Rc::clone(&changes));
        s.edit_audio(|page| page.set_sample_rate(22_050));
        assert_eq!(*changes.borrow(), 0);
        assert!(s.setup_error.contains("22050"));
        // With no device the readout shows that error instead of "No audio device running".
        assert_eq!(
            stats_text(&AudioStats::default(), &s.setup_error),
            s.setup_error
        );
    }

    #[test]
    fn no_audio_model_leaves_the_audio_tab_out_and_edits_nothing() {
        let changes = Rc::new(RefCell::new(0));
        let mut s = state(None, Rc::clone(&changes));
        assert!(!s.panel.tabs().iter().any(|t| t == "Audio"));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        assert_eq!(*changes.borrow(), 0);
    }

    #[test]
    fn picking_a_theme_selects_it_live_and_an_unknown_id_changes_nothing() {
        let changes = Rc::new(RefCell::new(0));
        let mut s = state(None, Rc::clone(&changes));
        let before = s.themes.active_id().map(str::to_owned);
        assert!(!s.pick_theme("no.such.theme"));
        assert_eq!(s.themes.active_id().map(str::to_owned), before);
        assert_eq!(*changes.borrow(), 0);

        assert!(s.pick_theme("openmixer.daylight"));
        assert_eq!(s.themes.active_id(), Some("openmixer.daylight"));
        assert_eq!(s.prefs.theme, "openmixer.daylight");
        assert_eq!(*changes.borrow(), 1);
    }

    #[test]
    fn the_accent_falls_back_to_the_surface_when_the_theme_has_none() {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        let mut tokens = themes.current().clone();
        tokens.accents.clear();
        assert_eq!(accent_of(&tokens), tokens.surface_raised);
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;
    use om_ui::settings::{MidiControl, PluginHost};
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::rc::Rc;

    /// A MIDI service that records loads and learn requests without touching a device.
    #[derive(Default)]
    struct FakeMidi {
        inputs: Vec<String>,
        learning: RefCell<Option<String>>,
        generation: Cell<u64>,
        fail_load: bool,
    }

    impl MidiInputs for FakeMidi {
        fn input_names(&self) -> Vec<String> {
            self.inputs.clone()
        }
    }

    impl MidiControl for FakeMidi {
        fn load_map(&self, _file: &Path) -> Result<(), String> {
            if self.fail_load {
                return Err("not a controller map".to_owned());
            }
            self.generation.set(self.generation.get() + 1);
            Ok(())
        }
        fn arm_learn(&self, target: &str) {
            *self.learning.borrow_mut() = Some(target.to_owned());
        }
        fn cancel_learn(&self) {
            *self.learning.borrow_mut() = None;
        }
        fn is_learning(&self) -> bool {
            self.learning.borrow().is_some()
        }
        fn map_generation(&self) -> u64 {
            self.generation.get()
        }
    }

    /// A library that keeps its folders in memory.
    #[derive(Default)]
    struct ListLibrary {
        folders: Vec<PathBuf>,
    }

    impl LibraryControl for ListLibrary {
        fn available(&self) -> bool {
            true
        }
        fn folders(&self) -> Vec<PathBuf> {
            self.folders.clone()
        }
        fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.folders.push(folder);
            Ok(())
        }
        fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.folders.retain(|f| *f != folder);
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            Ok(())
        }
        fn import_xml(&mut self, xml: PathBuf) -> SettingsResult<String> {
            Ok(format!("imported {}", xml.display()))
        }
    }

    /// Plugins that can be loaded and unloaded by id.
    #[derive(Default)]
    struct FakePlugins {
        loaded: RefCell<Vec<String>>,
    }

    impl PluginHost for FakePlugins {
        fn loaded_plugins(&self) -> Vec<String> {
            self.loaded.borrow().clone()
        }
        fn reload(&mut self) -> SettingsResult<()> {
            Ok(())
        }
        fn builtins(&self) -> Vec<(String, String)> {
            [("demo.a", "Demo A")]
                .into_iter()
                .filter(|(id, _)| !self.loaded.borrow().iter().any(|l| l == id))
                .map(|(id, name)| (id.to_owned(), name.to_owned()))
                .collect()
        }
        fn load_builtin(&mut self, id: &str) -> SettingsResult<()> {
            if id != "demo.a" {
                return Err(format!("no built-in plugin '{id}'"));
            }
            self.loaded.borrow_mut().push(id.to_owned());
            Ok(())
        }
        fn unload(&mut self, id: &str) -> SettingsResult<()> {
            self.loaded.borrow_mut().retain(|l| l != id);
            Ok(())
        }
    }

    fn options(
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
            plugins,
            sync_themes: Some(Box::new(move |_| syncs.set(syncs.get() + 1))),
        }
    }

    #[test]
    fn midi_learn_says_what_it_waits_for_and_what_it_got() {
        let fake = Arc::new(FakeMidi {
            inputs: vec!["Controller".into()],
            ..FakeMidi::default()
        });
        let midi: Arc<dyn MidiPage> = fake.clone();
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            Some(midi),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.refresh_lists();
        assert_eq!(s.lists.midi_inputs, vec!["Controller".to_owned()]);

        s.midi_learn("deck.0.play");
        assert_eq!(s.learning_target(), Some("deck.0.play"));
        assert!(s.notice.text.starts_with("Learning deck.0.play"));

        // The service learns a control: the map changes and the learn ends.
        *fake.learning.borrow_mut() = None;
        fake.generation.set(fake.generation.get() + 1);
        s.poll_learn();
        assert_eq!(s.learning_target(), None);
        assert_eq!(s.notice.text, "Learned a control for deck.0.play");

        s.midi_learn("deck.0.play");
        s.midi_cancel_learn();
        assert_eq!(s.learning_target(), None);
        assert_eq!(s.notice.text, "Learn cancelled");
        assert_eq!(changes.get(), 0, "learn edits no preference");
    }

    #[test]
    fn a_map_file_is_stored_only_when_it_loads_and_the_text_field_drives_it() {
        let fake: Arc<dyn MidiPage> = Arc::new(FakeMidi {
            fail_load: true,
            ..FakeMidi::default()
        });
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            Some(fake),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.begin_entry(EntryKind::MapFile);
        s.type_text("/maps/bad.json");
        s.submit_entry();
        assert!(s.notice.error);
        assert!(s.prefs.midi.map_file.is_empty());
        assert_eq!(changes.get(), 0);

        let good: Arc<dyn MidiPage> = Arc::new(FakeMidi::default());
        let mut s = SettingsState::new(options(
            Some(good),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.begin_entry(EntryKind::MapFile);
        s.type_text("/maps/good.json");
        s.backspace();
        s.type_text("n");
        s.submit_entry();
        assert_eq!(s.prefs.midi.map_file, "/maps/good.json");
        assert_eq!(changes.get(), 1);
        assert!(s.entry.is_none());
    }

    #[test]
    fn an_empty_path_keeps_the_field_open_and_cancel_closes_it() {
        let mut s = SettingsState::new(options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        ));
        s.begin_entry(EntryKind::WatchFolder);
        s.submit_entry();
        assert!(s.entry.is_some());
        assert!(s.notice.error);
        s.cancel_entry();
        assert!(s.entry.is_none());
    }

    #[test]
    fn watch_folders_follow_the_library_and_scan_settings_are_stored_without_one() {
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            None,
            Some(Box::new(ListLibrary::default())),
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.library_add_folder(PathBuf::from("/music"));
        assert_eq!(s.lists.folders, vec![PathBuf::from("/music")]);
        s.library_remove_folder(PathBuf::from("/music"));
        assert!(s.lists.folders.is_empty());
        s.library_import(PathBuf::from("/lib.xml"));
        assert_eq!(s.notice.text, "imported /lib.xml");

        let mut bare = SettingsState::new(options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        bare.library_set_interval(15);
        assert_eq!(bare.prefs.library.scan_interval_minutes, 15);
        bare.library_add_folder(PathBuf::from("/music"));
        assert!(bare.notice.error);
        assert!(bare.lists.folders.is_empty());
    }

    #[test]
    fn a_built_in_plugin_loads_and_unloads_and_the_themes_are_synced_each_time() {
        let syncs = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            None,
            None,
            Some(Box::new(FakePlugins::default())),
            Rc::clone(&syncs),
            Rc::new(Cell::new(0)),
        ));
        assert_eq!(
            s.lists.builtins,
            vec![("demo.a".to_owned(), "Demo A".to_owned())]
        );
        let after_start = syncs.get();

        s.plugin_load_builtin("demo.a");
        assert_eq!(s.lists.loaded_plugins, vec!["demo.a".to_owned()]);
        assert!(s.lists.builtins.is_empty());
        assert_eq!(s.notice.text, "Loaded demo.a");
        assert_eq!(syncs.get(), after_start + 1);

        s.plugin_unload("demo.a");
        assert!(s.lists.loaded_plugins.is_empty());
        assert_eq!(s.lists.builtins.len(), 1);
        s.plugin_load_builtin("missing");
        assert!(s.notice.error);
    }
}
