//! The MIDI, mapping and plugins pages' seam: what the settings overlay asks of the app and what it
//! shows. The binary owns the MIDI service, the mapper and the plugin registry, so it implements
//! [`DevicesHost`]; this crate carries the pages' data to the front end and one change back.
//! Every call returns the page as it stands (the front end draws only that), with a notice when the
//! change has something to say.

use serde::{Deserialize, Serialize};

/// One line of feedback the page shows under its title.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }
}

/// An id the host understands and the words the page shows for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

impl Choice {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// A page and what the last change said about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct View<P> {
    pub page: P,
    pub notice: Option<Notice>,
}

// ---- MIDI ----

/// The MIDI page: the inputs, the controller map in use and MIDI learn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiPage {
    /// False when no MIDI backend started (the page then says so).
    pub available: bool,
    pub inputs: Vec<String>,
    pub enable_all: bool,
    pub enabled_inputs: Vec<String>,
    /// The map file in use (the preference, or the default one).
    pub map_file: String,
    /// What MIDI learn can bind, with the words the page shows.
    pub targets: Vec<Choice>,
    /// The target an armed learn waits on.
    pub learning: Option<String>,
}

/// One edit of the MIDI page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum MidiChange {
    EnableAll(bool),
    ToggleInput(String),
    /// Loads a map file and keeps it as the map in use when it loads.
    LoadMap(String),
    LoadCurrentMap,
    UseDefaultMap,
    Learn(String),
    CancelLearn,
}

// ---- Mapping ----

/// What the binding table can be filtered by.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingFilter {
    pub text: String,
    pub group: Option<String>,
    pub channel: Option<i32>,
}

/// One row of the binding table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingRow {
    pub index: usize,
    pub text: String,
    pub group: String,
    pub conflicted: bool,
}

/// The edit form of one binding: every field as the text the person typed, and why the form does
/// not parse yet (null when it does).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingForm {
    /// A `kinds` id.
    pub kind: String,
    pub channel: String,
    pub number: String,
    pub target: String,
    /// A `modes` id.
    pub mode: String,
    pub min: String,
    pub max: String,
    pub step: String,
    pub initial: String,
    pub invert: bool,
    pub lsb: String,
    #[serde(skip_deserializing)]
    pub error: Option<String>,
}

/// Two bindings that listen to one control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingConflict {
    pub first: usize,
    pub second: usize,
    pub control: String,
}

/// The mapping editor: the binding table, the edit form, conflicts and the MIDI monitor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingPage {
    /// False without MIDI: the editor works on the live mapper.
    pub available: bool,
    pub map_file: String,
    /// Edits not saved to the map file yet.
    pub dirty: bool,
    pub learning: bool,
    pub filter: MappingFilter,
    pub groups: Vec<String>,
    pub rows: Vec<MappingRow>,
    pub selected: Option<usize>,
    pub form: Option<MappingForm>,
    pub conflicts: Vec<MappingConflict>,
    /// Newest first, each with its age.
    pub monitor: Vec<String>,
    pub kinds: Vec<Choice>,
    pub modes: Vec<Choice>,
    pub targets: Vec<Choice>,
}

/// One edit of the mapping page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum MappingChange {
    /// Re-reads the page only (the monitor and a finished learn).
    Refresh,
    SetFilter(MappingFilter),
    Select(Option<usize>),
    New,
    SetForm(Box<MappingForm>),
    Apply,
    Duplicate,
    Delete,
    LearnSelected,
    LearnNew(String),
    CancelLearn,
    Undo,
    Redo,
    Save,
    Import(String),
    Export(String),
    Reset,
    /// Keeps `first` or `second` of a conflict (`keep` is "first" or "second").
    Resolve {
        first: usize,
        second: usize,
        keep: String,
    },
}

// ---- Plugins ----

/// One parameter of a loaded plugin, as the generic panel shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginParam {
    pub id: String,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub value: f64,
    /// The value as the panel prints it.
    pub text: String,
}

/// One loaded plugin.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRow {
    pub id: String,
    /// `Latency 128 fr / 2.9 ms`; null when the plugin reports none.
    pub latency: Option<String>,
    /// The plugin has a window of its own to open.
    pub has_window: bool,
    pub params: Vec<PluginParam>,
}

/// What the page shows about CLAP plugins; absent in a build without CLAP.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClapSection {
    /// `unasked`, `allowed` or `declined`: may the standard CLAP folders be scanned.
    pub scan: String,
    pub standard_folders: Vec<String>,
    pub files: Vec<String>,
    /// `.clap` files that wait for the in-process warning.
    pub waiting: usize,
    pub warning_accepted: bool,
}

/// The plugins page: the plugin folder, what is loaded and the built-ins.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginsPage {
    pub available: bool,
    pub folder: String,
    pub loaded: Vec<PluginRow>,
    /// Built-in plugins that are not loaded: id and name.
    pub builtins: Vec<Choice>,
    pub clap: Option<ClapSection>,
    /// How many plugin themes Appearance lists.
    pub theme_count: usize,
}

/// One edit of the plugins page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum PluginsChange {
    /// Re-reads the page only (the latency readouts).
    Refresh,
    Reload,
    LoadBuiltin(String),
    Unload(String),
    SetParam {
        id: String,
        param: String,
        value: f64,
    },
    ClapScan(bool),
    ClapAcceptWarning,
}

/// What the app offers the MIDI, mapping and plugins pages.
pub trait DevicesHost {
    fn midi(&self) -> View<MidiPage>;
    fn change_midi(&self, change: MidiChange) -> View<MidiPage>;
    fn mapping(&self) -> View<MappingPage>;
    fn change_mapping(&self, change: MappingChange) -> View<MappingPage>;
    fn plugins(&self) -> View<PluginsPage>;
    fn change_plugins(&self, change: PluginsChange) -> View<PluginsPage>;
    /// The own window of loaded plugin `id`, when it has one.
    fn plugin_window(&self, id: &str) -> Option<std::sync::Arc<dyn om_ui::settings::PluginWindow>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_read_their_field_and_value() {
        let c: MidiChange = serde_json::from_str(r#"{"field":"toggleInput","value":"x"}"#).unwrap();
        assert_eq!(c, MidiChange::ToggleInput("x".into()));
        let c: MidiChange = serde_json::from_str(r#"{"field":"cancelLearn"}"#).unwrap();
        assert_eq!(c, MidiChange::CancelLearn);
        let c: MappingChange = serde_json::from_str(
            r#"{"field":"resolve","value":{"first":1,"second":2,"keep":"second"}}"#,
        )
        .unwrap();
        assert!(matches!(c, MappingChange::Resolve { second: 2, .. }));
        let c: PluginsChange = serde_json::from_str(
            r#"{"field":"setParam","value":{"id":"a","param":"b","value":0.5}}"#,
        )
        .unwrap();
        assert!(matches!(c, PluginsChange::SetParam { value, .. } if value == 0.5));
        assert!(serde_json::from_str::<MidiChange>(r#"{"field":"bogus"}"#).is_err());
    }

    #[test]
    fn a_form_ignores_an_error_the_front_end_sends_back() {
        let f: MappingForm = serde_json::from_str(
            r#"{"kind":"cc","channel":"1","number":"7","target":"t","mode":"absolute","min":"0","max":"1","step":"","initial":"","invert":true,"lsb":"","error":"stale"}"#,
        )
        .unwrap();
        assert_eq!(f.error, None);
        assert!(f.invert);
    }

    #[test]
    fn pages_serialise_in_camel_case() {
        let json = serde_json::to_string(&View::<MidiPage>::default()).unwrap();
        assert!(json.contains("\"enableAll\":false") && json.contains("\"learning\":null"));
    }
}
