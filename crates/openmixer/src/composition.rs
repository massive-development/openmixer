//! The device-free half of the app: engine, parameter router, theme registry, preferences and the
//! applier that pushes preferences into subsystems. Port of `src/app/AppComposition.cpp`. The
//! device owner (the binary, or the UI gate) takes the audio side with [`AppComposition::take_audio`]
//! and drives it, so headless and GUI runs share the same wiring.
//!
//! MIDI is attached on demand with [`AppComposition::attach_midi`], over a device backend (the
//! system one, or a mock). Plugins are owned by the composition ([`AppComposition::plugins`]): their
//! parameter targets route through the router, their stages insert into the engine, their maps
//! apply to the MIDI mapper and their themes reach the theme registry (see `crate::plugins`).
//! The library (Todo 13) is attached on demand with [`AppComposition::attach_library`].

use crate::app_targets::{
    AppPreferencesTargets, AppTargetsState, AudioDeviceControl, AudioSink, MidiSink, PluginSink,
};
use crate::control_timer::ControlTimer;
use crate::engine_state::EngineStateReader;
use crate::midi::{FeedbackThread, MidiService};
use om_audio::{DjEngine, DjEngineAudio};
use om_core::{ParameterRouter, StateReader};
use om_midi::MidiBackend;
use om_plugins::{InsertAt, InsertPoint, PluginError, PluginRegistry};
use om_ui::library::{default_options, LibraryServices, Options};
use om_ui::preferences::{self, Preferences};
use om_ui::preferences_applier::PreferencesApplier;
use om_ui::settings::PluginHost;
use om_ui::theme::{default_user_directory, session_theme_override, Source, ThemeRegistry};
use std::path::PathBuf;
use std::sync::Arc;

/// Number of decks the app starts with.
pub const DECKS: usize = 2;

/// Environment variable that selects a theme for the session (wins over the saved preference).
pub const THEME_ENV: &str = "OPENMIXER_THEME";

/// Optional hooks for subsystems that are attached on demand. Each is called from the matching
/// preference section; `None` means nothing is attached and the section is only stored.
#[derive(Default)]
pub struct ServiceHooks {
    /// The MIDI preference section, once [`AppComposition::attach_midi`] has run.
    pub midi: Option<Box<dyn MidiSink>>,
    /// The library (Todo 13): browser services, analysis and watch folders. Opened by
    /// [`AppComposition::attach_library`]; `None` until then.
    pub library: Option<LibraryServices>,
}

/// A theme registry with the built-in themes and, when given, the user theme folder. The window
/// keeps its own copy of this, loaded the same way.
pub fn load_themes(user_dir: Option<&std::path::Path>) -> ThemeRegistry {
    let mut themes = ThemeRegistry::new();
    themes.load_builtins();
    if let Some(dir) = user_dir {
        let loaded = themes.load_user_directory(dir);
        for error in &loaded.errors {
            eprintln!("openmixer: theme {error}");
        }
    }
    themes
}

/// Audio sink for plugin reloads, which never touch output routing.
struct NoAudio;

impl AudioSink for NoAudio {
    fn set_master_pair(&self, _pair: i32) {}
    fn set_cue_pair(&self, _pair: i32) {}
}

static NO_AUDIO: NoAudio = NoAudio;

pub struct AppComposition {
    pub engine: Arc<DjEngine>,
    pub router: Arc<ParameterRouter>,
    pub themes: ThemeRegistry,
    pub prefs: Preferences,
    /// False when the preferences file exists but could not be read or is from a newer schema:
    /// saving would replace data this build cannot read, so changes are not written.
    pub prefs_replaceable: bool,
    pub hooks: ServiceHooks,
    /// The plugins: loaded from the plugin folder by the preferences, or added in-process. Their
    /// parameter targets are routed through [`AppComposition::router`].
    pub plugins: PluginRegistry,
    /// The MIDI service, once attached: the settings page's controls and the input list.
    pub midi: Option<Arc<MidiService>>,
    /// Keeps the feedback thread running for as long as the MIDI service is attached.
    midi_feedback: Option<FeedbackThread>,
    /// Drives the engine's control clock (sync, slip, FX tempo) from its own thread.
    _control_timer: Option<ControlTimer>,
    applier: PreferencesApplier,
    targets: AppTargetsState,
    audio: Option<DjEngineAudio>,
}

impl AppComposition {
    /// Builds the engine and registers its parameters, loads the preferences file (a corrupt file
    /// falls back to defaults and is kept as `.bad`), and loads the built-in and user themes.
    /// Nothing is applied yet; call [`AppComposition::apply_preferences`] after the device exists.
    pub fn new(prefs_file: &std::path::Path, user_themes: Option<PathBuf>) -> Self {
        let (engine, audio) = DjEngine::new(DECKS);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        engine.register(&router);
        let mut plugins = PluginRegistry::new();
        plugins.bind_router(Some(Arc::clone(&router)));

        let (prefs, prefs_replaceable) = match Preferences::load(prefs_file) {
            Ok(p) => (p, true),
            Err(error) => {
                eprintln!("openmixer: {error}; using defaults");
                (Preferences::default(), error.allows_replacing_file())
            }
        };

        let themes = load_themes(user_themes.as_deref());
        let control_timer = match ControlTimer::start(Arc::clone(&engine)) {
            Ok(timer) => Some(timer),
            Err(error) => {
                eprintln!("openmixer: control clock thread: {error}");
                None
            }
        };

        Self {
            _control_timer: control_timer,
            engine,
            router,
            themes,
            prefs,
            prefs_replaceable,
            hooks: ServiceHooks::default(),
            plugins,
            midi: None,
            midi_feedback: None,
            applier: PreferencesApplier::new(),
            targets: AppTargetsState::default(),
            audio: Some(audio),
        }
    }

    /// Default composition: the preferences file from `OPENMIXER_PREFS` or the user config dir,
    /// and the user theme folder under the home directory.
    pub fn with_defaults() -> Self {
        let themes = default_user_directory(std::env::var_os("HOME").as_deref());
        Self::new(&preferences::default_file(), themes)
    }

    /// The audio side of the engine, once. The device owner passes it to its device.
    /// Opens the library at `db_path`, or at the default location (`OPENMIXER_LIBRARY_DB` or the user
    /// data folder) when `None`. The scan interval and scan-on-start come from the preferences and
    /// apply from this start on. A library that cannot open leaves the hook empty and returns why.
    pub fn attach_library(&mut self, db_path: Option<PathBuf>) -> Result<(), String> {
        let mut options = match db_path {
            Some(path) => Options::for_path(path.to_string_lossy().into_owned()),
            None => default_options(),
        };
        options.scan_on_start = self.prefs.library.scan_on_start;
        options.scan_interval_ms =
            i64::from(self.prefs.library.scan_interval_minutes.max(1)) * 60_000;
        options.on_error = Some(Box::new(|message| {
            eprintln!("openmixer: library: {message}")
        }));
        self.hooks.library = Some(LibraryServices::create(options)?);
        Ok(())
    }

    pub fn take_audio(&mut self) -> Option<DjEngineAudio> {
        self.audio.take()
    }

    /// Attaches MIDI over `backend`: parameter updates go to the router, feedback reads the engine,
    /// and the MIDI preference section now opens inputs and loads the map. The system backend, or a
    /// mock for headless runs. A second call replaces the first service.
    pub fn attach_midi(&mut self, backend: Box<dyn MidiBackend>) {
        let state: Arc<dyn StateReader> =
            Arc::new(EngineStateReader::new(Arc::clone(&self.engine)));
        let service = MidiService::new(backend, state);
        let target: Arc<dyn om_core::ParameterTarget> = self.router.clone();
        service.set_target(target);
        match service.start_feedback() {
            Ok(thread) => self.midi_feedback = Some(thread),
            Err(error) => eprintln!("openmixer: MIDI feedback thread: {error}"),
        }
        self.hooks.midi = Some(Box::new(Arc::clone(&service)));
        self.midi = Some(service);
    }

    /// Pushes every changed preference section to its subsystem (first call pushes everything).
    /// `audio` is the output routing (`OutputPairs` of the audio model) and `device` the open
    /// device, which is re-initialised only for a stored state it does not already have.
    /// `started_with` is the state the app opened the device with at start-up, if any, so that
    /// state is not initialised a second time.
    pub fn apply_preferences(
        &mut self,
        audio: &dyn AudioSink,
        device: Option<&mut dyn AudioDeviceControl>,
        started_with: Option<&str>,
    ) -> usize {
        let plugins: &mut dyn PluginSink = &mut self.plugins;
        let mut targets = AppPreferencesTargets::new(
            &mut self.targets,
            &mut self.themes,
            audio,
            device,
            self.hooks.midi.as_deref_mut(),
            Some(plugins),
        );
        if let Some(state) = started_with {
            targets.note_audio_requested(state);
        }
        let pushed = self.applier.apply(&mut targets, &self.prefs);
        // OPENMIXER_THEME wins for the session; the saved preference is kept on disk.
        let env = std::env::var(THEME_ENV).ok();
        if let Some(theme) = session_theme_override(env.as_deref()) {
            if !self.themes.select(theme) {
                eprintln!("openmixer: {THEME_ENV} theme '{theme}' not found");
            }
        }
        pushed
    }

    /// Creates the plugin stage `stage_id` and inserts it into the engine at `at`/`slot` (channel
    /// or master). The stage stays until its plugin unloads or [`AppComposition::detach_plugin_stage`].
    pub fn attach_plugin_stage(
        &mut self,
        stage_id: &str,
        at: InsertAt,
        slot: usize,
    ) -> Result<(), PluginError> {
        let point: Arc<dyn InsertPoint> = self.engine.mixer_handle();
        self.plugins.attach_stage(stage_id, &point, at, slot)
    }

    /// Clears a slot a plugin stage was attached to.
    pub fn detach_plugin_stage(&mut self, at: InsertAt, slot: usize) {
        let point: Arc<dyn InsertPoint> = self.engine.mixer_handle();
        self.plugins.detach_stage(&point, at, slot);
    }

    /// Applies a plugin's controller map to the MIDI mapper, replacing the live map. Fails when
    /// MIDI is not attached.
    pub fn apply_plugin_midi_map(&self, map_name: &str) -> Result<(), String> {
        let service = self.midi.as_ref().ok_or("MIDI is not attached")?;
        self.plugins
            .apply_midi_map(map_name, service.mapper())
            .map_err(|e| e.to_string())
    }

    /// One line on MIDI for the smoke report: the open inputs, the mappings and the devices listed.
    pub fn midi_status(&self) -> String {
        match &self.midi {
            Some(midi) => format!(
                "{} input(s) open, {} mapping(s), {} device(s) listed",
                midi.open_inputs().len(),
                midi.mapper().num_mappings(),
                midi.available_inputs().len()
            ),
            None => "not available".to_owned(),
        }
    }

    /// One line on plugins for the smoke report: the loaded plugins, the built-ins not loaded and
    /// the plugin themes.
    pub fn plugins_status(&self) -> String {
        let loaded = self.plugins.plugin_ids();
        let names = if loaded.is_empty() {
            "none".to_owned()
        } else {
            loaded.join(", ")
        };
        let available = PluginHost::builtins(self).len();
        let themes = self
            .themes
            .themes()
            .iter()
            .filter(|e| e.source == Source::Plugin)
            .count();
        format!(
            "{} loaded ({names}), {available} built-in not loaded, {themes} plugin theme(s)",
            loaded.len()
        )
    }

    /// Reloads the plugin folder applied last (the Plugins page's Reload button). Themes are
    /// re-forwarded, and the selection is restored when its plugin theme is back.
    pub fn reload_plugins(&mut self) {
        let mut targets = AppPreferencesTargets::new(
            &mut self.targets,
            &mut self.themes,
            &NO_AUDIO,
            None,
            self.hooks.midi.as_deref_mut(),
            Some(&mut self.plugins),
        );
        targets.reload_plugins();
    }
}
