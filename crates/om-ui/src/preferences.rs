//! User preferences (Todo 29): one plain model, versioned JSON on disk. Port of `src/ui/Preferences.cpp`.
//!
//! Same schema, field names and migration rules as the C++ app: unknown or bad fields fall back to
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
pub const PREFERENCES_VERSION: i64 = 1;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    pub theme: String,
    pub audio: AudioPrefs,
    pub midi: MidiPrefs,
    pub library: LibraryPrefs,
    /// Empty = the default plugin folder.
    pub plugin_folder: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "openmixer.dark".to_owned(),
            audio: AudioPrefs::default(),
            midi: MidiPrefs::default(),
            library: LibraryPrefs::default(),
            plugin_folder: String::new(),
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
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("om-ui-prefs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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
            r#"{"version":2}"#,
            r#"{"version":"1"}"#,
        ] {
            assert!(Preferences::from_json(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn save_and_load_through_a_file() {
        let dir = scratch("roundtrip");
        let file = dir.join("nested").join("preferences.json");
        customised().save(&file).unwrap();
        assert_eq!(Preferences::load(&file).unwrap(), customised());
    }

    #[test]
    fn missing_file_means_defaults() {
        let dir = scratch("missing");
        assert_eq!(
            Preferences::load(&dir.join("none.json")).unwrap(),
            Preferences::default()
        );
    }

    #[test]
    fn corrupt_file_is_kept_as_bad_and_the_next_save_replaces_it() {
        let dir = scratch("corrupt");
        let file = dir.join("preferences.json");
        fs::write(&file, "{ broken").unwrap();
        assert!(Preferences::load(&file).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("preferences.json.bad")).unwrap(),
            "{ broken"
        );
        Preferences::default().save(&file).unwrap();
        assert_eq!(Preferences::load(&file).unwrap(), Preferences::default());
    }

    #[test]
    fn an_unreadable_file_is_never_replaced_but_a_corrupt_one_may_be() {
        let dir = scratch("unreadable");
        let file = dir.join("preferences.json");
        let bytes = [0xff_u8, 0xfe, 0x00];
        fs::write(&file, bytes).unwrap();
        let error = Preferences::load(&file).unwrap_err();
        assert!(matches!(error, PreferencesError::Io(_)), "{error}");
        assert!(!error.allows_replacing_file(), "a save would destroy the file");
        assert_eq!(fs::read(&file).unwrap(), bytes, "load leaves it alone");

        fs::write(&file, "{ broken").unwrap();
        let corrupt = Preferences::load(&file).unwrap_err();
        assert!(corrupt.allows_replacing_file(), "kept as .bad, so replacing it loses nothing");

        fs::write(&file, r#"{"version": 99}"#).unwrap();
        let newer = Preferences::load(&file).unwrap_err();
        assert!(!newer.allows_replacing_file(), "a newer schema is not ours to overwrite");
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
