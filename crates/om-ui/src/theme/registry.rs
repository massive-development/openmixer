//! Theme registry: built-in, plugin-provided and user themes, with precedence and live switching.
//!
//! Precedence for one id: User > Plugin > BuiltIn; within a source the latest add wins. Selecting a
//! theme makes it the current theme. Global current-theme state and its listener list are
//! replaced by an owned registry (one per app) and a [`ThemeRegistry::generation`] counter that
//! the UI compares to notice a change.

use super::builtin::BUILTIN_THEMES;
use super::json::{parse_theme, parse_theme_file, ParsedTheme, ThemeError, ThemeInfo};
use super::tokens::ThemeTokens;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where a theme came from. Higher wins when two sources define the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    BuiltIn = 0,
    Plugin = 1,
    User = 2,
}

/// One registered theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub info: ThemeInfo,
    pub tokens: ThemeTokens,
    pub source: Source,
    /// Plugin id or file name; lets one provider be removed as a unit.
    pub owner: String,
}

/// Result of rescanning the user theme directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserLoad {
    pub loaded: usize,
    /// `file name: reason` for every file that was skipped.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeRegistry {
    entries: Vec<Entry>,
    active_id: Option<String>,
    current: ThemeTokens,
    generation: u64,
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeRegistry {
    /// The id selected when nothing else is, and the fallback when the active theme disappears.
    pub const DEFAULT_ID: &'static str = "openmixer.dark";

    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            active_id: None,
            current: ThemeTokens::builtin_dark(),
            generation: 0,
        }
    }

    /// Tokens of the active theme; the built-in dark theme until something is selected.
    pub fn current(&self) -> &ThemeTokens {
        &self.current
    }

    /// Id of the active theme, if one was selected.
    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    /// Increases whenever the active tokens or the registry contents change.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Parses `json` and registers it. A bad document leaves the registry untouched.
    pub fn add_json(&mut self, json: &str, source: Source, owner: &str) -> Result<(), ThemeError> {
        let parsed = parse_theme(json)?;
        self.add_parsed(parsed, source, owner);
        Ok(())
    }

    /// Registers tokens that code built directly.
    pub fn add(&mut self, info: ThemeInfo, tokens: ThemeTokens, source: Source, owner: &str) {
        self.add_entry(Entry {
            info,
            tokens,
            source,
            owner: owner.to_owned(),
        });
        self.entries_changed();
    }

    fn add_parsed(&mut self, parsed: ParsedTheme, source: Source, owner: &str) {
        self.add_entry(Entry {
            info: parsed.info,
            tokens: parsed.tokens,
            source,
            owner: owner.to_owned(),
        });
        self.entries_changed();
    }

    /// Same id from the same provider replaces; different providers stack and precedence picks.
    fn add_entry(&mut self, entry: Entry) {
        self.entries.retain(|e| {
            !(e.info.id == entry.info.id && e.source == entry.source && e.owner == entry.owner)
        });
        self.entries.push(entry);
    }

    /// Registers the themes shipped inside the binary. Returns how many loaded.
    pub fn load_builtins(&mut self) -> usize {
        let mut loaded = 0;
        for (owner, json) in BUILTIN_THEMES {
            if self.add_json(json, Source::BuiltIn, owner).is_ok() {
                loaded += 1;
            }
        }
        loaded
    }

    /// Replaces every user theme with the valid `*.json` files in `dir`, in file-name order. A bad
    /// file is skipped and reported; a missing directory is simply empty.
    pub fn load_user_directory(&mut self, dir: &Path) -> UserLoad {
        self.entries.retain(|e| e.source != Source::User);
        let mut out = UserLoad::default();
        for path in json_files(dir) {
            let owner = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match parse_theme_file(&path) {
                Ok(parsed) => {
                    self.add_entry(Entry {
                        info: parsed.info,
                        tokens: parsed.tokens,
                        source: Source::User,
                        owner,
                    });
                    out.loaded += 1;
                }
                Err(e) => out.errors.push(format!("{owner}: {e}")),
            }
        }
        self.entries_changed();
        out
    }

    /// Removes everything one provider registered.
    pub fn remove_owner(&mut self, owner: &str) {
        self.entries.retain(|e| e.owner != owner);
        self.entries_changed();
    }

    /// Removes every theme from one source.
    pub fn remove_source(&mut self, source: Source) {
        self.entries.retain(|e| e.source != source);
        self.entries_changed();
    }

    /// The winning entry for `id`: highest source, and among equals the latest add.
    pub fn find(&self, id: &str) -> Option<&Entry> {
        let mut best: Option<&Entry> = None;
        for e in self.entries.iter().filter(|e| e.info.id == id) {
            if best.is_none_or(|b| e.source >= b.source) {
                best = Some(e);
            }
        }
        best
    }

    /// One entry per id after precedence, sorted by id.
    pub fn themes(&self) -> Vec<&Entry> {
        let mut by_id: BTreeMap<&str, &Entry> = BTreeMap::new();
        for e in &self.entries {
            if let Some(winner) = self.find(&e.info.id) {
                by_id.insert(e.info.id.as_str(), winner);
            }
        }
        by_id.into_values().collect()
    }

    /// Makes `id` the current theme. `false` (and no change) when the id is unknown.
    pub fn select(&mut self, id: &str) -> bool {
        let Some(tokens) = self.find(id).map(|e| normalised(e.tokens.clone())) else {
            return false;
        };
        self.active_id = Some(id.to_owned());
        self.current = tokens;
        self.generation += 1;
        true
    }

    /// The active theme may have been overridden or removed: re-apply it, else fall back to the
    /// default, else to the built-in dark tokens.
    fn entries_changed(&mut self) {
        if let Some(active) = self.active_id.clone() {
            if self.find(&active).is_none() {
                self.active_id = self
                    .find(Self::DEFAULT_ID)
                    .map(|_| Self::DEFAULT_ID.to_owned());
            }
            self.current = match self.active_id.as_deref().and_then(|id| self.find(id)) {
                Some(entry) => normalised(entry.tokens.clone()),
                None => ThemeTokens::builtin_dark(),
            };
        }
        self.generation += 1;
    }
}

/// The current theme always has a palette: an empty one takes the built-in accents.
fn normalised(mut tokens: ThemeTokens) -> ThemeTokens {
    if tokens.accents.is_empty() {
        tokens.accents = ThemeTokens::builtin_dark().accents;
    }
    tokens
}

/// `*.json` files directly inside `dir`, sorted by name. Empty when the directory is absent.
fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}

/// Default user theme directory, under the user application data directory on Linux
/// (`$HOME/.config`) plus `OpenMixer/themes`. `None` when `HOME` is unset.
pub fn default_user_directory(home: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    home.map(|h| {
        PathBuf::from(h)
            .join(".config")
            .join("OpenMixer")
            .join("themes")
    })
}

/// The session theme override: a non-empty `OPENMIXER_THEME` value wins for the session. The
/// caller applies it with [`ThemeRegistry::select`] after the saved preference.
pub fn session_theme_override(env_value: Option<&str>) -> Option<&str> {
    env_value.filter(|v| !v.is_empty())
}
