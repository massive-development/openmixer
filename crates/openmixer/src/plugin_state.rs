//! The plugins' saved state, beside the preferences (ADR 0007, step 6).
//!
//! `<preferences folder>/plugin-state.json` holds one opaque blob per plugin id, as the plugin saved
//! it. It is a file of its own, not a section of `preferences.json`, because a plugin's state can be
//! megabytes and the preferences are rewritten on every settings change. The pattern is the sample
//! bank's `sampler.json`: a schema version, a corrupt file kept as `.bad`, a file from a newer
//! version read as empty and never overwritten, and saves written to a temp file and renamed.
//!
//! ```text
//! {"version": 1, "plugins": [{"id": "org.example.gain", "state": "<base64>"}]}
//! ```

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use om_plugins::MAX_STATE_BYTES;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// File name beside the preferences file.
pub const STATE_FILE: &str = "plugin-state.json";
/// Schema version written to disk. Higher versions are refused and left untouched.
pub const STATE_VERSION: i64 = 1;

/// Plugin id to the state bytes the plugin saved.
pub type PluginStates = BTreeMap<String, Vec<u8>>;

/// What reading the file found: the states, and a line for everything skipped or set aside.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    pub states: PluginStates,
    pub issues: Vec<String>,
}

/// The state file next to `prefs_file`.
pub fn state_file_for(prefs_file: &Path) -> PathBuf {
    prefs_file
        .parent()
        .map_or_else(|| PathBuf::from(STATE_FILE), |dir| dir.join(STATE_FILE))
}

/// Reads and writes one state file. Remembers what it last saw, so an unchanged set is not written
/// again, and whether the file may be replaced.
#[derive(Debug)]
pub struct PluginStateStore {
    path: PathBuf,
    /// False when the file is from a newer version (or unreadable): saving would destroy data this
    /// build cannot read.
    writable: bool,
    last: PluginStates,
}

impl PluginStateStore {
    /// A store over `path`; nothing is read until [`Self::load`].
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            writable: true,
            last: PluginStates::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the file. A missing file is empty. A corrupt file is copied to `<name>.bad` and read as
    /// empty (the next save replaces it); a newer version or an unreadable file is read as empty and
    /// never overwritten. Bad entries are skipped with an issue.
    pub fn load(&mut self) -> Loaded {
        self.writable = true;
        let mut loaded = Loaded::default();
        if self.path.is_file() {
            match fs::read_to_string(&self.path) {
                Ok(text) => match parse(&text) {
                    Ok(parsed) => loaded = parsed,
                    Err(Refusal::Newer(version)) => {
                        self.writable = false;
                        loaded.issues.push(format!(
                            "{}: version {version} is newer than this build reads; plugin state is not saved",
                            self.path.display()
                        ));
                    }
                    Err(Refusal::Corrupt(reason)) => {
                        let mut bad = self.path.as_os_str().to_owned();
                        bad.push(".bad");
                        let kept = match fs::copy(&self.path, PathBuf::from(&bad)) {
                            Ok(_) => "; kept as .bad",
                            Err(_) => "; could not keep a .bad copy",
                        };
                        loaded
                            .issues
                            .push(format!("{}: {reason}{kept}", self.path.display()));
                    }
                },
                Err(error) => {
                    self.writable = false;
                    loaded.issues.push(format!(
                        "{}: {error}; plugin state is not saved",
                        self.path.display()
                    ));
                }
            }
        }
        self.last.clone_from(&loaded.states);
        loaded
    }

    /// Writes `states`, creating the folder, through a temp file renamed into place. Does nothing
    /// when the set equals what was last read or written. Refuses (`Ok(false)`) when the file must
    /// not be replaced. Entries over [`MAX_STATE_BYTES`] are dropped.
    pub fn save(&mut self, states: &PluginStates) -> std::io::Result<bool> {
        if !self.writable {
            return Ok(false);
        }
        let kept: PluginStates = states
            .iter()
            .filter(|(id, bytes)| !id.is_empty() && bytes.len() <= MAX_STATE_BYTES)
            .map(|(id, bytes)| (id.clone(), bytes.clone()))
            .collect();
        if kept == self.last {
            return Ok(false);
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = self.path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, render(&kept))?;
        fs::rename(&tmp, &self.path)?;
        self.last = kept;
        Ok(true)
    }
}

enum Refusal {
    Newer(i64),
    Corrupt(String),
}

fn render(states: &PluginStates) -> String {
    let plugins: Vec<Value> = states
        .iter()
        .map(|(id, bytes)| json!({ "id": id, "state": STANDARD.encode(bytes) }))
        .collect();
    json!({ "version": STATE_VERSION, "plugins": plugins }).to_string()
}

fn parse(text: &str) -> Result<Loaded, Refusal> {
    let root: Value = serde_json::from_str(text).map_err(|e| Refusal::Corrupt(e.to_string()))?;
    let object = root
        .as_object()
        .ok_or_else(|| Refusal::Corrupt("not a JSON object".into()))?;
    match object.get("version").and_then(Value::as_i64) {
        Some(STATE_VERSION) => {}
        Some(newer) if newer > STATE_VERSION => return Err(Refusal::Newer(newer)),
        _ => return Err(Refusal::Corrupt("no usable version".into())),
    }
    let mut loaded = Loaded::default();
    let Some(items) = object.get("plugins").and_then(Value::as_array) else {
        loaded
            .issues
            .push("plugins is not a list; no state loaded".into());
        return Ok(loaded);
    };
    for (index, item) in items.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let text = item.get("state").and_then(Value::as_str);
        let (Some(id), Some(text)) = (id, text) else {
            loaded.issues.push(format!(
                "entry {index}: needs a string id and state; skipped"
            ));
            continue;
        };
        // Base64 grows the data by a third: a text over the cap's encoded size cannot fit.
        if text.len() > MAX_STATE_BYTES / 3 * 4 + 4 {
            loaded
                .issues
                .push(format!("{id}: state is too large; skipped"));
            continue;
        }
        match STANDARD.decode(text) {
            Ok(bytes) if bytes.len() <= MAX_STATE_BYTES => {
                if loaded.states.insert(id.to_owned(), bytes).is_some() {
                    loaded
                        .issues
                        .push(format!("{id}: listed twice; kept the last"));
                }
            }
            Ok(_) => loaded
                .issues
                .push(format!("{id}: state is too large; skipped")),
            Err(error) => loaded
                .issues
                .push(format!("{id}: state is not base64 ({error}); skipped")),
        }
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states() -> PluginStates {
        PluginStates::from([
            ("a.plugin".to_owned(), vec![0, 1, 2, 255]),
            ("b.plugin".to_owned(), Vec::new()),
        ])
    }

    fn store(dir: &tempfile::TempDir) -> PluginStateStore {
        PluginStateStore::new(dir.path().join("sub").join(STATE_FILE))
    }

    #[test]
    fn states_round_trip_and_the_folder_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = store(&dir);
        assert_eq!(first.load(), Loaded::default(), "a missing file is empty");
        assert!(first.save(&states()).unwrap());
        assert!(
            !first.save(&states()).unwrap(),
            "unchanged is not rewritten"
        );
        let loaded = store(&dir).load();
        assert_eq!(loaded.states, states());
        assert!(loaded.issues.is_empty());
        let text = fs::read_to_string(first.path()).unwrap();
        assert!(text.contains("\"version\":1"), "{text}");
        assert!(!first.path().with_extension("json.tmp").exists());
    }

    #[test]
    fn a_corrupt_file_is_kept_as_bad_and_replaced_by_the_next_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(&dir);
        fs::create_dir_all(s.path().parent().unwrap()).unwrap();
        fs::write(s.path(), "{ nope").unwrap();
        let loaded = s.load();
        assert!(loaded.states.is_empty());
        assert!(loaded.issues[0].contains(".bad"), "{:?}", loaded.issues);
        assert!(s.path().with_extension("json.bad").is_file());
        assert!(s.save(&states()).unwrap());
        assert_eq!(store(&dir).load().states, states());
    }

    #[test]
    fn a_newer_version_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(&dir);
        fs::create_dir_all(s.path().parent().unwrap()).unwrap();
        let newer = r#"{"version":2,"plugins":[]}"#;
        fs::write(s.path(), newer).unwrap();
        let loaded = s.load();
        assert!(loaded.states.is_empty());
        assert!(loaded.issues[0].contains("newer"), "{:?}", loaded.issues);
        assert!(!s.save(&states()).unwrap());
        assert_eq!(fs::read_to_string(s.path()).unwrap(), newer);
        assert!(!s.path().with_extension("json.bad").exists());
    }

    #[test]
    fn bad_entries_are_skipped_with_an_issue_and_good_ones_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(&dir);
        fs::create_dir_all(s.path().parent().unwrap()).unwrap();
        fs::write(
            s.path(),
            r#"{"version":1,"plugins":[
                {"id":"ok","state":"AQID"},
                {"id":"bad64","state":"!!!"},
                {"id":"","state":"AQID"},
                {"state":"AQID"},
                {"id":"ok","state":"BAUG"},
                5]}"#,
        )
        .unwrap();
        let loaded = s.load();
        assert_eq!(
            loaded.states,
            PluginStates::from([("ok".to_owned(), vec![4, 5, 6])])
        );
        assert_eq!(loaded.issues.len(), 5, "{:?}", loaded.issues);
    }

    #[test]
    fn an_oversized_state_is_dropped_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(&dir);
        let mut big = states();
        big.insert("big".into(), vec![0; MAX_STATE_BYTES + 1]);
        assert!(s.save(&big).unwrap());
        assert_eq!(store(&dir).load().states, states());
    }

    #[test]
    fn the_file_sits_beside_the_preferences() {
        assert_eq!(
            state_file_for(Path::new("/home/u/.config/OpenMixer/preferences.json")),
            PathBuf::from("/home/u/.config/OpenMixer/plugin-state.json")
        );
    }
}
