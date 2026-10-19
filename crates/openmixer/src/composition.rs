//! The device-free half of the app: engine, parameter router, theme registry, preferences and the
//! applier that pushes preferences into subsystems. Port of `src/app/AppComposition.cpp`. The
//! device owner (the binary, or the UI gate) takes the audio side with [`AppComposition::take_audio`]
//! and drives it, so headless and GUI runs share the same wiring.
//!
//! Pieces that depend on crates not yet ported are optional hooks in [`ServiceHooks`]: MIDI
//! (Todo 10, `om-midi`) and plugins (Todo 14, `om-plugins`). Until those exist the defaults are
//! no-ops, and the matching preference sections are stored but not acted on. The library (Todo 13)
//! is attached on demand with [`AppComposition::attach_library`].

use crate::app_targets::{
    AppPreferencesTargets, AppTargetsState, AudioDeviceControl, AudioSink, MidiSink, PluginSink,
};
use om_audio::{DjEngine, DjEngineAudio};
use om_core::ParameterRouter;
use om_ui::library::{default_options, LibraryServices, Options};
use om_ui::preferences::{self, Preferences};
use om_ui::preferences_applier::PreferencesApplier;
use om_ui::theme::{default_user_directory, session_theme_override, ThemeRegistry};
use std::path::PathBuf;
use std::sync::Arc;

/// Number of decks the app starts with.
pub const DECKS: usize = 2;

/// Environment variable that selects a theme for the session (wins over the saved preference).
pub const THEME_ENV: &str = "OPENMIXER_THEME";

/// Optional hooks for subsystems that are not ported yet. Each is called from the matching
/// preference section; `None` means nothing is attached and the section is only stored.
#[derive(Default)]
pub struct ServiceHooks {
    /// Placeholder for `om-midi` (Todo 10): receives the MIDI preference section.
    pub midi: Option<Box<dyn MidiSink>>,
    /// Placeholder for `om-plugins` (Todo 14): loads the plugin folder.
    pub plugins: Option<Box<dyn PluginSink>>,
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

pub struct AppComposition {
    pub engine: Arc<DjEngine>,
    pub router: Arc<ParameterRouter>,
    pub themes: ThemeRegistry,
    pub prefs: Preferences,
    pub hooks: ServiceHooks,
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

        let prefs = match Preferences::load(prefs_file) {
            Ok(p) => p,
            Err(error) => {
                eprintln!("openmixer: {error}; using defaults");
                Preferences::default()
            }
        };

        let themes = load_themes(user_themes.as_deref());

        Self {
            engine,
            router,
            themes,
            prefs,
            hooks: ServiceHooks::default(),
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
        options.scan_interval_ms = i64::from(self.prefs.library.scan_interval_minutes.max(1)) * 60_000;
        options.on_error = Some(Box::new(|message| eprintln!("openmixer: library: {message}")));
        self.hooks.library = Some(LibraryServices::create(options)?);
        Ok(())
    }

    pub fn take_audio(&mut self) -> Option<DjEngineAudio> {
        self.audio.take()
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
        let mut targets = AppPreferencesTargets::new(
            &mut self.targets,
            &mut self.themes,
            audio,
            device,
            self.hooks.midi.as_deref_mut(),
            self.hooks.plugins.as_deref_mut(),
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
}
