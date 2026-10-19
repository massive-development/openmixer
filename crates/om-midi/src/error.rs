use std::path::PathBuf;

/// Failures from map and trace parsing, file access and MIDI devices.
#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    /// A map, trace or binding that does not satisfy the schema; the message says which rule.
    #[error("{0}")]
    Invalid(String),
    #[error("file not found: {}", .0.display())]
    NotFound(PathBuf),
    /// The MIDI system is unavailable, or a port cannot be opened.
    #[error("MIDI device: {0}")]
    Device(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
