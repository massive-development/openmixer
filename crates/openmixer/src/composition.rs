//! The device-free half of the app: engine, parameter router, theme registry, preferences and the
//! applier that pushes preferences into subsystems. The
//! device owner (the binary, or the UI gate) takes the audio side with [`AppComposition::take_audio`]
//! and drives it, so headless and GUI runs share the same wiring.
//!
//! MIDI is attached on demand with [`AppComposition::attach_midi`], over a device backend (the
//! system one, or a mock). Plugins are owned by the composition ([`AppComposition::plugins`]): their
//! parameter targets route through the router, their stages insert into the engine, their maps
//! apply to the MIDI mapper and their themes reach the theme registry (see `crate::plugins`).
//! The library is attached on demand with [`AppComposition::attach_library`].

use crate::app_targets::{
    AppPreferencesTargets, AppTargetsState, AudioDeviceControl, AudioSink, PluginSink,
};
use crate::control_timer::ControlTimer;
use crate::engine_state::EngineStateReader;
use crate::error::AppError;
use crate::midi::{FeedbackThread, MidiService};
use crate::stems_host::StemsHost;
use om_audio::{DjEngine, DjEngineAudio};
use om_core::{ParameterRouter, StateReader};
use om_midi::MidiBackend;
use om_plugins::{InsertAt, InsertPoint, PluginError, PluginRegistry};
use om_stems::{ModelManifest, StemSeparator, StemService, WeightManager};
use om_ui::library::{default_options, LibraryServices, Options};
use om_ui::preferences::{self, Preferences};
use om_ui::preferences_applier::PreferencesApplier;
use om_ui::settings::{MidiHost, PluginHost};
use om_ui::theme::{default_user_directory, session_theme_override, Source, ThemeRegistry};
use std::path::PathBuf;
use std::sync::Arc;

/// Number of decks the app starts with.
pub const DECKS: usize = 2;

/// Stem model manifest, in the `models` folder beside the library. Written by the model install
/// step (S4); absent means stem separation stays off.
pub const STEM_MANIFEST_FILE: &str = "htdemucs.manifest.json";
/// The separator's id and version: part of every stem cache key.
pub const STEM_SEPARATOR_ID: &str = "htdemucs";
pub const STEM_SEPARATOR_VERSION: i32 = 1;
const GIB: u64 = 1024 * 1024 * 1024;

/// Environment variable that selects a theme for the session (wins over the saved preference).
pub const THEME_ENV: &str = "OPENMIXER_THEME";

/// The subsystems attached on demand. Each preference section drives the matching service; `None`
/// means nothing is attached and the section is only stored.
#[derive(Default)]
pub struct Services {
    /// The library: browser services, analysis and watch folders. Opened by
    /// [`AppComposition::attach_library`]; `None` until then.
    pub library: Option<LibraryServices>,
    /// The MIDI service, once [`AppComposition::attach_midi`] has run: the preferences, the
    /// settings page and the input list all reach it.
    pub midi: Option<Arc<MidiService>>,
    /// The stem service, once [`AppComposition::attach_stems`] found a separator (an installed
    /// model with the `onnx` feature, or the mock). Jobs only start when something submits them.
    pub stems: Option<Arc<StemService>>,
}

impl Services {
    /// The MIDI service as the preferences and the settings page drive it.
    pub fn midi_host(&self) -> Option<&dyn MidiHost> {
        self.midi.as_deref().map(|midi| midi as &dyn MidiHost)
    }
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
    pub services: Services,
    /// The plugins: loaded from the plugin folder by the preferences, or added in-process. Their
    /// parameter targets are routed through [`AppComposition::router`].
    pub plugins: PluginRegistry,
    /// Keeps the feedback thread running for as long as the MIDI service is attached.
    midi_feedback: Option<FeedbackThread>,
    /// Drives the engine's control clock (sync, slip, FX tempo) from its own thread.
    _control_timer: Option<ControlTimer>,
    applier: PreferencesApplier,
    targets: AppTargetsState,
    audio: Option<DjEngineAudio>,
    /// The library file path, once [`AppComposition::attach_library`] has opened it.
    library_db: Option<String>,
    /// The stems host, until the window takes it.
    stems_host: Option<StemsHost>,
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
            services: Services::default(),
            plugins,
            midi_feedback: None,
            applier: PreferencesApplier::new(),
            targets: AppTargetsState::default(),
            audio: Some(audio),
            library_db: None,
            stems_host: None,
        }
    }

    /// Default composition: the preferences file from `OPENMIXER_PREFS` or the user config dir,
    /// and the user theme folder under the home directory.
    pub fn with_defaults() -> Self {
        let home = dirs::home_dir().map(std::path::PathBuf::into_os_string);
        let themes = default_user_directory(home.as_deref());
        Self::new(&preferences::default_file(), themes)
    }

    /// The audio side of the engine, once. The device owner passes it to its device.
    /// Opens the library at `db_path`, or at the default location (`OPENMIXER_LIBRARY_DB` or the user
    /// data folder) when `None`. The scan interval and scan-on-start come from the preferences and
    /// apply from this start on. A library that cannot open leaves the hook empty and returns why.
    pub fn attach_library(&mut self, db_path: Option<PathBuf>) -> Result<(), AppError> {
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
        self.library_db = Some(options.db_path.clone());
        self.services.library = Some(LibraryServices::create(options)?);
        Ok(())
    }

    /// Attaches the stems: the host (model state, licence, download, cache) and, when the model is
    /// ready now, the stem service in `services.stems`. The cap comes from `stems.max_cache_gb`.
    /// With `mock`, the mock separator stands in for an installed model. A model that is not
    /// installed is not an error: the host says so and the service is created later, on the first
    /// job after an install. The host goes to the window with [`AppComposition::take_stems_host`].
    pub fn attach_stems(&mut self, mock: bool) -> Result<(), AppError> {
        let db = self
            .library_db
            .clone()
            .ok_or_else(|| AppError::Stems("attach the library first".into()))?;
        let max_cache_bytes = u64::try_from(self.prefs.stems.max_cache_gb).unwrap_or(0) * GIB;
        let mut host = StemsHost::new(mock, db, Some(max_cache_bytes));
        self.services.stems = host.service_if_ready()?;
        self.stems_host = Some(host);
        Ok(())
    }

    /// The stems host, once [`AppComposition::attach_stems`] has run. Taken by the window.
    pub fn take_stems_host(&mut self) -> Option<StemsHost> {
        self.stems_host.take()
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
        self.services.midi = Some(service);
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
            self.services.midi_host(),
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
    pub fn apply_plugin_midi_map(&self, map_name: &str) -> Result<(), AppError> {
        let service = self
            .services
            .midi
            .as_ref()
            .ok_or(AppError::MidiNotAttached)?;
        Ok(self.plugins.apply_midi_map(map_name, service.mapper())?)
    }

    /// One line on MIDI for the smoke report: the open inputs, the mappings and the devices listed.
    pub fn midi_status(&self) -> String {
        match &self.services.midi {
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
            self.services.midi_host(),
            Some(&mut self.plugins),
        );
        targets.reload_plugins();
    }
}

/// The htdemucs separator over ONNX Runtime, when the model is installed and the `onnx` feature is
/// built in. `None` (with a line on stderr) otherwise.
#[cfg(feature = "onnx")]
pub(crate) fn installed_separator(
    weights: &WeightManager,
    manifest: &ModelManifest,
) -> Result<Option<Arc<dyn StemSeparator>>, AppError> {
    use om_stems::{default_threads, HtDemucsSeparator, OrtInference};
    if !weights.is_installed(manifest) {
        eprintln!(
            "openmixer: stem model {} is not installed; stems are off",
            manifest.id
        );
        return Ok(None);
    }
    let contract = manifest.contract_or_default();
    let inference = OrtInference::load(
        &weights.model_file(manifest),
        contract.clone(),
        Some(default_threads()),
    )
    .map_err(|e| AppError::Stems(e.to_string()))?;
    let separator = HtDemucsSeparator::new(
        STEM_SEPARATOR_ID,
        STEM_SEPARATOR_VERSION,
        inference,
        contract,
    )
    .map_err(|e| AppError::Stems(e.to_string()))?;
    Ok(Some(Arc::new(separator)))
}

#[cfg(not(feature = "onnx"))]
pub(crate) fn installed_separator(
    _weights: &WeightManager,
    _manifest: &ModelManifest,
) -> Result<Option<Arc<dyn StemSeparator>>, AppError> {
    eprintln!("openmixer: built without the onnx feature; stem separation is off");
    Ok(None)
}
