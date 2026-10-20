//! User preferences: one plain model, versioned JSON on disk.
//!
//! Schema, field names and migration rules are kept stable: unknown or bad fields fall back to
//! their default one by one; a file from a newer schema (or not a JSON object) is refused and the
//! target is left untouched; a corrupt file is kept as `<name>.bad` and the defaults are used until
//! the next save. No UI or subsystem knowledge here: `PreferencesApplier` pushes values into
//! subsystems.

use serde_json::{Map, Value};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Schema version written to disk. Files with a higher version are refused.
///
/// History: 1 = audio, midi, library, plugins. 2 adds `stems` (cache cap and auto-prepare). 3 adds
/// `automix`. Each step is additive: an older file has no new section, so it reads with the
/// defaults and is written back as the current version on the next save.
pub const PREFERENCES_VERSION: i64 = 3;

/// Automix settings (Wave 7). Read when the window starts and when the Automix page changes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomixPrefs {
    /// Automix runs. Off by default: nothing is ever mixed without the user turning it on.
    pub enabled: bool,
    /// Style code (`AutomixStyle::as_u8`): 0 Auto, others per the style list. Clamped to 0..=5 (5 is Stem bridge).
    pub style: i32,
    /// Transition length in beats: 8, 16 or 32 (other values snap to the nearest).
    pub length_beats: i32,
    /// Queue source: a playlist id, or 0 for the whole library. Ignored while `queue_search` is set.
    pub queue_playlist: i64,
    /// Queue source is the library browser's current search (live, not saved as text). Read as
    /// false when missing, so version 3 files from before this field keep their playlist source.
    pub queue_search: bool,
    /// Smart pick: prefer harmonic and tempo matches among the queue candidates.
    pub smart: bool,
    /// Largest tempo gap a beat-matched style accepts, in percent (0..=20).
    pub min_bpm_gap_percent: i32,
    /// Start the next track by itself when the playing one nears its end.
    pub autostart_at_end: bool,
}

pub const MAX_AUTOMIX_STYLE: i64 = 5;
pub const MAX_BPM_GAP_PERCENT: i64 = 20;

impl Default for AutomixPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            style: 0,
            length_beats: 16,
            queue_playlist: 0,
            queue_search: false,
            smart: true,
            min_bpm_gap_percent: 6,
            autostart_at_end: true,
        }
    }
}

/// Snaps a requested transition length to 8, 16 or 32 beats (the nearest; ties go shorter).
pub fn snap_length_beats(beats: i64) -> i32 {
    [8i64, 16, 32]
        .into_iter()
        .min_by_key(|candidate| ((candidate - beats).abs(), *candidate))
        .unwrap_or(16) as i32
}
/// Largest stem cache cap accepted, in GB.
pub const MAX_STEM_CACHE_GB: i64 = 1000;
/// Environment variable that overrides the preferences file location.
pub const PREFS_ENV: &str = "OPENMIXER_PREFS";

const MAX_PAIR: i64 = 31;
const MAX_INTERVAL_MINUTES: i64 = 24 * 60;

/// Audio section. `device_state` is the device manager's state (type, device, rate, buffer,
/// channels) as an opaque string; empty means the system default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPrefs {
    pub device_state: String,
    /// Device output pair carrying the master mix (pair p = channels 2p, 2p+1).
    pub master_pair: i32,
    /// Headphone cue pair; -1 = none.
    pub cue_pair: i32,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        Self {
            device_state: String::new(),
            master_pair: 0,
            cue_pair: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPrefs {
    pub enable_all: bool,
    /// Device identifiers, used when `enable_all` is false.
    pub enabled_inputs: Vec<String>,
    /// Empty = the default map file.
    pub map_file: String,
}

impl Default for MidiPrefs {
    fn default() -> Self {
        Self {
            enable_all: true,
            enabled_inputs: Vec::new(),
            map_file: String::new(),
        }
    }
}

/// Read when the library services are created (restart to change).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPrefs {
    pub scan_on_start: bool,
    pub scan_interval_minutes: i32,
}

impl Default for LibraryPrefs {
    fn default() -> Self {
        Self {
            scan_on_start: true,
            scan_interval_minutes: 5,
        }
    }
}

/// Stem separation (Wave 7). Read when the stem service starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StemsPrefs {
    /// Cap for the stem cache, in GB (1..=1000). Least recently used stems are removed above it.
    pub max_cache_gb: i32,
    /// Prepare stems for a track automatically. Off by default; loading a track never starts a
    /// separation on its own, whatever this says.
    pub auto_prepare: bool,
    /// Load a track's cached stems into its deck after the track loads (when the deck is not
    /// playing). On by default; a track without cached stems loads plain.
    pub auto_load: bool,
}

impl Default for StemsPrefs {
    fn default() -> Self {
        Self {
            max_cache_gb: 20,
            auto_prepare: false,
            auto_load: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    pub theme: String,
    pub audio: AudioPrefs,
    pub midi: MidiPrefs,
    pub library: LibraryPrefs,
    /// Empty = the default plugin folder.
    pub plugin_folder: String,
    pub stems: StemsPrefs,
    pub automix: AutomixPrefs,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "openmixer.dark".to_owned(),
            audio: AudioPrefs::default(),
            midi: MidiPrefs::default(),
            library: LibraryPrefs::default(),
            plugin_folder: String::new(),
            stems: StemsPrefs::default(),
            automix: AutomixPrefs::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PreferencesError {
    #[error("preferences: not a JSON object")]
    NotAnObject,
    #[error("preferences: unsupported version {0}")]
    UnsupportedVersion(i64),
    #[error("preferences: invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("preferences: {0}")]
    Io(#[from] std::io::Error),
}

impl PreferencesError {
    /// True when a save may replace the file that failed to load. A corrupt file has been copied to
    /// `.bad`, so replacing it loses nothing. An unreadable file or one from a newer schema holds
    /// data this build cannot read, so it must be left alone.
    pub fn allows_replacing_file(&self) -> bool {
        matches!(self, Self::Json(_))
    }
}

/// `<user config>/OpenMixer`, the folder that holds preferences, plugins and the map by default.
pub fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OpenMixer")
}

/// The preferences file: `OPENMIXER_PREFS` when set and non-empty, else `<data dir>/preferences.json`.
pub fn default_file() -> PathBuf {
    default_file_from(std::env::var_os(PREFS_ENV).as_deref())
}

/// [`default_file`] with the environment value passed in (testable without touching the process env).
pub fn default_file_from(env: Option<&OsStr>) -> PathBuf {
    match env {
        Some(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => data_dir().join("preferences.json"),
    }
}

pub fn default_plugin_folder() -> PathBuf {
    data_dir().join("plugins")
}

pub fn default_map_file() -> PathBuf {
    data_dir().join("controller_map.json")
}

/// Where a learned binding is saved when no map file is set: the user map beside the preferences.
pub fn default_user_map_file() -> PathBuf {
    data_dir().join("midi-user-map.json")
}

impl Preferences {
    pub fn effective_plugin_folder(&self) -> PathBuf {
        if self.plugin_folder.is_empty() {
            default_plugin_folder()
        } else {
            PathBuf::from(&self.plugin_folder)
        }
    }

    pub fn effective_map_file(&self) -> PathBuf {
        if self.midi.map_file.is_empty() {
            default_map_file()
        } else {
            PathBuf::from(&self.midi.map_file)
        }
    }

    /// Serialises every field, compact, with the schema version.
    pub fn to_json(&self) -> String {
        let mut audio = Map::new();
        audio.insert("deviceState".into(), self.audio.device_state.clone().into());
        audio.insert("masterPair".into(), self.audio.master_pair.into());
        audio.insert("cuePair".into(), self.audio.cue_pair.into());

        let mut midi = Map::new();
        midi.insert("enableAll".into(), self.midi.enable_all.into());
        midi.insert(
            "enabledInputs".into(),
            Value::Array(
                self.midi
                    .enabled_inputs
                    .iter()
                    .cloned()
                    .map(Value::from)
                    .collect(),
            ),
        );
        midi.insert("mapFile".into(), self.midi.map_file.clone().into());

        let mut library = Map::new();
        library.insert("scanOnStart".into(), self.library.scan_on_start.into());
        library.insert(
            "scanIntervalMinutes".into(),
            self.library.scan_interval_minutes.into(),
        );

        let mut plugins = Map::new();
        plugins.insert("folder".into(), self.plugin_folder.clone().into());

        let mut root = Map::new();
        root.insert("version".into(), PREFERENCES_VERSION.into());
        root.insert("theme".into(), self.theme.clone().into());
        root.insert("audio".into(), Value::Object(audio));
        root.insert("midi".into(), Value::Object(midi));
        root.insert("library".into(), Value::Object(library));
        root.insert("plugins".into(), Value::Object(plugins));
        let mut stems = Map::new();
        stems.insert("maxCacheGb".into(), self.stems.max_cache_gb.into());
        stems.insert("autoPrepare".into(), self.stems.auto_prepare.into());
        stems.insert("autoLoad".into(), self.stems.auto_load.into());
        root.insert("stems".into(), Value::Object(stems));
        let mut automix = Map::new();
        automix.insert("enabled".into(), self.automix.enabled.into());
        automix.insert("style".into(), self.automix.style.into());
        automix.insert("lengthBeats".into(), self.automix.length_beats.into());
        automix.insert("queuePlaylist".into(), self.automix.queue_playlist.into());
        automix.insert("queueSearch".into(), self.automix.queue_search.into());
        automix.insert("smart".into(), self.automix.smart.into());
        automix.insert(
            "minBpmGapPercent".into(),
            self.automix.min_bpm_gap_percent.into(),
        );
        automix.insert(
            "autostartAtEnd".into(),
            self.automix.autostart_at_end.into(),
        );
        root.insert("automix".into(), Value::Object(automix));
        Value::Object(root).to_string()
    }

    /// Parses `json` over the defaults. Fails (leaving nothing) for non-JSON, a non-object, or a
    /// schema version outside `1..=PREFERENCES_VERSION`.
    pub fn from_json(json: &str) -> Result<Self, PreferencesError> {
        let root: Value = serde_json::from_str(json)?;
        let Some(root) = root.as_object() else {
            return Err(PreferencesError::NotAnObject);
        };
        let version = int_or(root.get("version"), 0, 1_000_000, 0);
        if !(1..=PREFERENCES_VERSION).contains(&version) {
            return Err(PreferencesError::UnsupportedVersion(version));
        }

        let mut p = Self::default();
        p.theme = string_or(root.get("theme"), p.theme);
        read_audio(section(root, "audio"), &mut p.audio);
        read_midi(section(root, "midi"), &mut p.midi);
        read_library(section(root, "library"), &mut p.library);
        p.plugin_folder = string_or(section(root, "plugins").get("folder"), p.plugin_folder);
        read_stems(section(root, "stems"), &mut p.stems);
        read_automix(section(root, "automix"), &mut p.automix);
        Ok(p)
    }

    /// A missing file means defaults. A corrupt file is copied to `<name>.bad` and the error is
    /// returned; the caller uses `Preferences::default()` in that case, and the next save replaces
    /// the broken file.
    pub fn load(path: &Path) -> Result<Self, PreferencesError> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)?;
        match Self::from_json(&text) {
            Ok(p) => Ok(p),
            Err(error) => {
                let mut bad = path.as_os_str().to_owned();
                bad.push(".bad");
                fs::copy(path, PathBuf::from(bad))?;
                Err(error)
            }
        }
    }

    /// Writes the file, creating its folder. Written to a sibling temp file and renamed into place.
    pub fn save(&self, path: &Path) -> Result<(), PreferencesError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, self.to_json())?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

fn section<'a>(root: &'a Map<String, Value>, name: &str) -> &'a Map<String, Value> {
    static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
    root.get(name)
        .and_then(Value::as_object)
        .unwrap_or_else(|| EMPTY.get_or_init(Map::new))
}

fn string_or(v: Option<&Value>, fallback: String) -> String {
    v.and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or(fallback)
}

fn bool_or(v: Option<&Value>, fallback: bool) -> bool {
    v.and_then(Value::as_bool).unwrap_or(fallback)
}

/// Integer in `lo..=hi`, otherwise the fallback. Floats and strings are not integers.
fn int_or(v: Option<&Value>, lo: i64, hi: i64, fallback: i64) -> i64 {
    v.and_then(Value::as_i64)
        .filter(|n| (lo..=hi).contains(n))
        .unwrap_or(fallback)
}

fn read_audio(s: &Map<String, Value>, a: &mut AudioPrefs) {
    a.device_state = string_or(s.get("deviceState"), a.device_state.clone());
    a.master_pair = int_or(s.get("masterPair"), 0, MAX_PAIR, a.master_pair.into()) as i32;
    a.cue_pair = int_or(s.get("cuePair"), -1, MAX_PAIR, a.cue_pair.into()) as i32;
}

fn read_midi(s: &Map<String, Value>, m: &mut MidiPrefs) {
    m.enable_all = bool_or(s.get("enableAll"), m.enable_all);
    m.map_file = string_or(s.get("mapFile"), m.map_file.clone());
    if let Some(list) = s.get("enabledInputs").and_then(Value::as_array) {
        m.enabled_inputs.clear();
        for item in list {
            if let Some(name) = item.as_str() {
                if !m.enabled_inputs.iter().any(|existing| existing == name) {
                    m.enabled_inputs.push(name.to_owned());
                }
            }
        }
    }
}

fn read_stems(s: &Map<String, Value>, st: &mut StemsPrefs) {
    st.max_cache_gb = int_or(
        s.get("maxCacheGb"),
        1,
        MAX_STEM_CACHE_GB,
        st.max_cache_gb.into(),
    ) as i32;
    st.auto_prepare = bool_or(s.get("autoPrepare"), st.auto_prepare);
    st.auto_load = bool_or(s.get("autoLoad"), st.auto_load);
}

/// Reads the automix section. Every value is clamped or defaulted on its own, so a hostile file
/// (negative, huge, wrong type) never reaches the controller unchecked.
fn read_automix(s: &Map<String, Value>, a: &mut AutomixPrefs) {
    a.enabled = bool_or(s.get("enabled"), a.enabled);
    a.style = int_or(s.get("style"), 0, MAX_AUTOMIX_STYLE, a.style.into()) as i32;
    a.length_beats = snap_length_beats(int_or(
        s.get("lengthBeats"),
        1,
        1_000,
        a.length_beats.into(),
    ));
    a.queue_playlist = int_or(s.get("queuePlaylist"), 0, i64::MAX, a.queue_playlist);
    a.queue_search = bool_or(s.get("queueSearch"), a.queue_search);
    a.smart = bool_or(s.get("smart"), a.smart);
    a.min_bpm_gap_percent = int_or(
        s.get("minBpmGapPercent"),
        0,
        MAX_BPM_GAP_PERCENT,
        a.min_bpm_gap_percent.into(),
    ) as i32;
    a.autostart_at_end = bool_or(s.get("autostartAtEnd"), a.autostart_at_end);
}

fn read_library(s: &Map<String, Value>, l: &mut LibraryPrefs) {
    l.scan_on_start = bool_or(s.get("scanOnStart"), l.scan_on_start);
    l.scan_interval_minutes = int_or(
        s.get("scanIntervalMinutes"),
        1,
        MAX_INTERVAL_MINUTES,
        l.scan_interval_minutes.into(),
    ) as i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customised() -> Preferences {
        Preferences {
            theme: "openmixer.daylight".into(),
            audio: AudioPrefs {
                device_state: "<DEVICESETUP deviceType=\"ALSA\" audioOutputDeviceName=\"hw:1\"/>"
                    .into(),
                master_pair: 2,
                cue_pair: -1,
            },
            midi: MidiPrefs {
                enable_all: false,
                enabled_inputs: vec!["usb-1".into(), "usb-2".into()],
                map_file: "/maps/my map.json".into(),
            },
            library: LibraryPrefs {
                scan_on_start: false,
                scan_interval_minutes: 15,
            },
            plugin_folder: "/opt/openmixer/plugins".into(),
            stems: StemsPrefs {
                max_cache_gb: 64,
                auto_prepare: true,
                auto_load: false,
            },
            automix: AutomixPrefs {
                enabled: true,
                style: 2,
                length_beats: 32,
                queue_playlist: 7,
                queue_search: true,
                smart: false,
                min_bpm_gap_percent: 3,
                autostart_at_end: false,
            },
        }
    }

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn defaults_are_sensible() {
        let p = Preferences::default();
        assert_eq!(p.theme, "openmixer.dark");
        assert!(p.midi.enable_all);
        assert_eq!(p.audio.master_pair, 0);
        assert_eq!(p.audio.cue_pair, 1);
        assert!(p.library.scan_on_start);
        assert_eq!(p.library.scan_interval_minutes, 5);
        assert_eq!(p.effective_plugin_folder(), default_plugin_folder());
        assert_eq!(p.effective_map_file(), default_map_file());
        assert_eq!(
            p.stems,
            StemsPrefs {
                max_cache_gb: 20,
                auto_prepare: false,
                auto_load: true,
            }
        );
    }

    #[test]
    fn version_one_files_read_with_stem_defaults() {
        let p = Preferences::from_json(
            r#"{"version":1,"theme":"openmixer.dark","library":{"scanOnStart":false}}"#,
        )
        .unwrap();
        assert!(!p.library.scan_on_start);
        assert_eq!(p.stems, StemsPrefs::default());
    }

    #[test]
    fn version_two_files_read_with_automix_defaults() {
        let p = Preferences::from_json(r#"{"version":2,"stems":{"autoLoad":false}}"#).unwrap();
        assert_eq!(p.automix, AutomixPrefs::default());
        assert!(!p.automix.enabled);
        assert_eq!(p.automix.length_beats, 16);
    }

    #[test]
    fn stem_bridge_style_survives_a_reload() {
        let p = Preferences::from_json(r#"{"version":3,"automix":{"style":5}}"#).unwrap();
        assert_eq!(p.automix.style, 5);
    }

    #[test]
    fn hostile_automix_values_fall_back_or_snap() {
        let p = Preferences::from_json(
            r#"{"version":3,"automix":{"enabled":"yes","style":99,"lengthBeats":-4,
                "queuePlaylist":-1,"minBpmGapPercent":500,"smart":3}}"#,
        )
        .unwrap();
        let a = &p.automix;
        assert!(!a.enabled);
        assert_eq!(a.style, 0);
        assert_eq!(
            a.length_beats, 16,
            "-4 is out of range and falls back to the default"
        );
        assert_eq!(a.queue_playlist, 0);
        assert_eq!(a.min_bpm_gap_percent, 6);
        assert!(a.smart);
        assert_eq!(snap_length_beats(20), 16);
        assert_eq!(snap_length_beats(1_000), 32);
    }

    #[test]
    fn stem_fields_fall_back_when_out_of_range() {
        let p =
            Preferences::from_json(r#"{"version":2,"stems":{"maxCacheGb":0,"autoPrepare":"yes"}}"#)
                .unwrap();
        assert_eq!(p.stems.max_cache_gb, 20);
        assert!(!p.stems.auto_prepare);
        assert!(p.stems.auto_load, "a missing key keeps the default");
        let p = Preferences::from_json(r#"{"version":2,"stems":{"maxCacheGb":5000}}"#).unwrap();
        assert_eq!(p.stems.max_cache_gb, 20);
    }

    #[test]
    fn json_round_trips_every_field() {
        let original = customised();
        let loaded = Preferences::from_json(&original.to_json()).unwrap();
        assert_eq!(loaded, original);
        assert_eq!(
            loaded.effective_plugin_folder(),
            PathBuf::from("/opt/openmixer/plugins")
        );
        assert_eq!(loaded.to_json(), original.to_json());
    }

    #[test]
    fn json_carries_the_schema_version() {
        let root: Value = serde_json::from_str(&Preferences::default().to_json()).unwrap();
        assert_eq!(root["version"], PREFERENCES_VERSION);
    }

    #[test]
    fn missing_sections_and_bad_fields_fall_back_one_by_one() {
        let p = Preferences::from_json(
            r#"{"version":1,"theme":"x","audio":{"masterPair":"two","cuePair":99999},
                "midi":{"enableAll":"yes","enabledInputs":["a",3,"a","b"]},
                "library":{"scanIntervalMinutes":0}}"#,
        )
        .unwrap();
        assert_eq!(p.theme, "x");
        assert_eq!(p.audio.master_pair, 0, "wrong type keeps the default");
        assert_eq!(p.audio.cue_pair, 1, "out of range keeps the default");
        assert!(p.midi.enable_all);
        assert_eq!(p.midi.enabled_inputs, vec!["a", "b"]);
        assert_eq!(p.library.scan_interval_minutes, 5);
        assert_eq!(p.plugin_folder, "");
    }

    #[test]
    fn rejects_non_json_and_unsupported_versions() {
        for bad in [
            "",
            "not json",
            "[]",
            "{}",
            r#"{"version":0}"#,
            r#"{"version":4}"#,
            r#"{"version":"1"}"#,
        ] {
            assert!(Preferences::from_json(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn save_and_load_through_a_file() {
        let dir = scratch();
        let file = dir.path().join("nested").join("preferences.json");
        customised().save(&file).unwrap();
        assert_eq!(Preferences::load(&file).unwrap(), customised());
    }

    #[test]
    fn missing_file_means_defaults() {
        let dir = scratch();
        assert_eq!(
            Preferences::load(&dir.path().join("none.json")).unwrap(),
            Preferences::default()
        );
    }

    #[test]
    fn corrupt_file_is_kept_as_bad_and_the_next_save_replaces_it() {
        let dir = scratch();
        let file = dir.path().join("preferences.json");
        fs::write(&file, "{ broken").unwrap();
        assert!(Preferences::load(&file).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("preferences.json.bad")).unwrap(),
            "{ broken"
        );
        Preferences::default().save(&file).unwrap();
        assert_eq!(Preferences::load(&file).unwrap(), Preferences::default());
    }

    #[test]
    fn an_unreadable_file_is_never_replaced_but_a_corrupt_one_may_be() {
        let dir = scratch();
        let file = dir.path().join("preferences.json");
        let bytes = [0xff_u8, 0xfe, 0x00];
        fs::write(&file, bytes).unwrap();
        let error = Preferences::load(&file).unwrap_err();
        assert!(matches!(error, PreferencesError::Io(_)), "{error}");
        assert!(
            !error.allows_replacing_file(),
            "a save would destroy the file"
        );
        assert_eq!(fs::read(&file).unwrap(), bytes, "load leaves it alone");

        fs::write(&file, "{ broken").unwrap();
        let corrupt = Preferences::load(&file).unwrap_err();
        assert!(
            corrupt.allows_replacing_file(),
            "kept as .bad, so replacing it loses nothing"
        );

        fs::write(&file, r#"{"version": 99}"#).unwrap();
        let newer = Preferences::load(&file).unwrap_err();
        assert!(
            !newer.allows_replacing_file(),
            "a newer schema is not ours to overwrite"
        );
    }

    #[test]
    fn default_file_honours_the_environment_override() {
        assert_eq!(
            default_file_from(Some(OsStr::new("/tmp/om_prefs_env/p.json"))),
            PathBuf::from("/tmp/om_prefs_env/p.json")
        );
        assert_eq!(
            default_file_from(Some(OsStr::new(""))),
            data_dir().join("preferences.json")
        );
        assert!(default_file_from(None).ends_with("OpenMixer/preferences.json"));
    }
}
