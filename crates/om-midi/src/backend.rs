//! Device layer: what the MIDI routers need from the host's MIDI system. Enumerate inputs and
//! outputs, open an input with a byte callback, open an output, and report hot-plug changes.
//!
//! The concrete backends are [`crate::AlsaBackend`] (Linux, on the sequencer) and
//! [`crate::MockBackend`] (tests). Platforms without a backend get [`system_backend`]'s error; a
//! midir backend plugs in there for macOS and Windows (see `docs/spikes/DECISIONS.md`).

use crate::error::MidiError;

/// One MIDI port as the host names it. `id` is stable across restarts (it is what a preference
/// stores); `name` is what the user sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
}

/// Receives the raw bytes of each complete message from an input, on the backend's thread.
/// Must not block: it runs on the device's thread.
pub type MidiCallback = Box<dyn FnMut(&[u8]) + Send>;

/// An open input. Dropping it stops the callback.
pub trait MidiInputConnection: Send {}

/// An open output. Sends are control-thread work and may block briefly.
pub trait MidiOutputPort: Send {
    fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError>;
}

pub trait MidiBackend: Send {
    /// Short name for diagnostics, such as "alsa" or "mock".
    fn name(&self) -> &'static str;

    /// Input ports the host can see now.
    fn inputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError>;

    /// Output ports the host can see now.
    fn outputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError>;

    /// Opens the input port `id` and calls `callback` for each message.
    fn open_input(
        &mut self,
        id: &str,
        callback: MidiCallback,
    ) -> Result<Box<dyn MidiInputConnection>, MidiError>;

    /// Opens the output port `id`.
    fn open_output(&mut self, id: &str) -> Result<Box<dyn MidiOutputPort>, MidiError>;

    /// Re-reads the port lists. True when the set of inputs or outputs changed since the last
    /// refresh (the first refresh reports true). Hot-plug handling polls this.
    fn refresh(&mut self) -> Result<bool, MidiError>;
}

/// The last port lists a backend reported, for [`MidiBackend::refresh`] to compare against.
#[derive(Debug, Default)]
pub struct PortSnapshot {
    last: Option<(Vec<MidiPortInfo>, Vec<MidiPortInfo>)>,
}

impl PortSnapshot {
    /// Records the lists and returns whether they differ from the previous ones.
    pub fn update(&mut self, inputs: Vec<MidiPortInfo>, outputs: Vec<MidiPortInfo>) -> bool {
        let changed = self.last.as_ref() != Some(&(inputs.clone(), outputs.clone()));
        self.last = Some((inputs, outputs));
        changed
    }
}

/// The backend for this platform. On Linux it is the ALSA sequencer; it fails when the sequencer
/// is not present (no `/dev/snd/seq`), and the app then runs without MIDI.
pub fn system_backend() -> Result<Box<dyn MidiBackend>, MidiError> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(crate::alsa_backend::AlsaBackend::open()?))
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Documented stub. The midir backend goes here for macOS and Windows: it implements
        // `MidiBackend` the same way (one client per routed direction, hot-plug through
        // `refresh`). Until then these platforms run without MIDI.
        Err(MidiError::Device(
            "no MIDI backend for this platform yet".to_owned(),
        ))
    }
}
