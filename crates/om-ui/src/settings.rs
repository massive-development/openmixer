//! Settings model: the logic behind the settings overlay. The non-drawing parts: the audio, MIDI, library, appearance and plugin pages.
//!
//! Pages edit the shared [`Preferences`] and report whether an edit changed anything; the caller
//! saves and applies (see `openmixer::composition`). The MIDI, library and plugin pages talk to
//! traits that the app implements over `om-midi`, `om-library` and `om-plugins`. Drawing lives in `om-ui-gpui`.

use om_library::LibraryError;

mod appearance;
mod audio;
mod library;
mod midi;
mod panel;
mod plugins;
mod stems;

pub use appearance::AppearanceSettingsPage;
pub use audio::{stats_text, AudioSettingsModel, AudioSettingsPage, AudioStats};
pub use library::{LibraryControl, LibrarySettingsPage};
pub use midi::{learnable_targets, MidiHost, MidiInputs, MidiPage, MidiSettingsPage};
pub use panel::{PageKind, SettingsPanel};
pub use plugins::{PluginHost, PluginsSettingsPage};
pub use stems::{format_gib, StemsSettingsPage};

/// An error from a layer the settings page does not name (the audio device, the MIDI service, the
/// plugin host). The owning crate's error goes in boxed; its display text is shown as is.
pub type OpaqueError = Box<dyn std::error::Error + Send + Sync>;

/// Why a settings change was refused. The display text is shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The audio layer refused the change, or the device would not open with it.
    #[error("{0}")]
    Audio(#[source] OpaqueError),
    /// The MIDI service refused to load or save the controller map.
    #[error("{0}")]
    Midi(#[source] OpaqueError),
    /// The plugin host failed to reload or to load or unload a plugin.
    #[error("{0}")]
    Plugin(#[source] OpaqueError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error("The library is not available")]
    LibraryUnavailable,
    #[error("no watch folders available")]
    NoWatchFolders,
    #[error("import is not available")]
    ImportUnavailable,
    #[error("Plugins are not available")]
    PluginsUnavailable,
    #[error("stem separation is not available")]
    StemsUnavailable,
    /// The model is not ready; the text says why (it is shown in the Library page).
    #[error("{0}")]
    StemsNotReady(String),
    /// A stored value does not fit the field it is written to.
    #[error("{field} is out of range")]
    InvalidValue { field: &'static str },
    #[error("unknown theme '{id}'")]
    UnknownTheme { id: String },
    #[error("no built-in plugin '{id}'")]
    NoBuiltinPlugin { id: String },
    #[error("plugin '{id}' is not loaded")]
    PluginNotLoaded { id: String },
}

/// Result of a settings change. `Ok(true)` when the stored value changed.
pub type SettingsResult<T = bool> = Result<T, SettingsError>;

/// Assigns and reports whether the value changed.
fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}
