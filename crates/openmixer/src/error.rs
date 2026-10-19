//! The app's error type: the subsystem errors the composition, the MIDI and audio glue, and the
//! smoke checks can fail with. Display texts are the ones the status lines show.

use om_audio::AudioError;
use om_library::LibraryError;
use om_midi::MidiError;
use om_plugins::PluginError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Midi(#[from] MidiError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Load(#[from] om_ui::library::LoadError),
    /// A device change failed and the previous setup could not be reopened either.
    #[error("{error}; restoring the previous setup failed: {restore}")]
    AudioRestore {
        error: AudioError,
        restore: AudioError,
    },
    #[error("no audio device is open")]
    NoAudioDevice,
    #[error("unknown device type '{0}'")]
    UnknownDeviceType(String),
    #[error("unreadable audio device state")]
    UnreadableAudioState,
    #[error("MIDI is not attached")]
    MidiNotAttached,
    /// A smoke check that does not carry numbers.
    #[error("{0}")]
    Smoke(&'static str),
    #[error("rendered {rendered} of {expected} blocks")]
    SmokeRendered { rendered: usize, expected: usize },
    #[error("{0} xruns")]
    SmokeXruns(u64),
    #[error("only {count} of {expected} tracks were ingested")]
    SmokeTracks { count: usize, expected: usize },
    #[error("the table shows {rows} rows, not {expected}")]
    SmokeRows { rows: usize, expected: usize },
}
