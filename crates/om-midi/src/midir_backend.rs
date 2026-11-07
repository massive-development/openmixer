//! The `midir` backend: ALSA sequencer on Linux, CoreMIDI on macOS, WinMM on Windows. Each routed
//! direction is its own midir client named `openmixer`; an input's callback runs on midir's thread.
//!
//! On Linux midir names a port `Client:Port client:port`, where the trailing `client:port` is the
//! sequencer address and changes when a device is replugged. A port's id is that name with the
//! address stripped (`Launchpad Mini:Launchpad Mini MIDI 1`), so a saved preference survives a
//! replug. Two identical devices therefore share an id, and the first one wins on open.

use crate::backend::{
    MidiBackend, MidiCallback, MidiInputConnection, MidiOutputPort, MidiPortInfo, PortSnapshot,
};
use crate::error::MidiError;
use midir::{Ignore, MidiIO, MidiInput, MidiOutput};

/// The client name every midir client of this app uses. Enumeration skips the ports it owns.
const CLIENT_NAME: &str = "openmixer";
/// The kernel's loopback client. It is both an input and an output, so listing it would feed
/// every message straight back into the app.
const MIDI_THROUGH: &str = "Midi Through";

/// `name` without midir's trailing ` client:port` sequencer address, when it has one.
fn strip_address(name: &str) -> &str {
    let Some((head, tail)) = name.rsplit_once(' ') else {
        return name;
    };
    match tail.split_once(':') {
        Some((client, port))
            if !client.is_empty()
                && !port.is_empty()
                && client.bytes().all(|b| b.is_ascii_digit())
                && port.bytes().all(|b| b.is_ascii_digit()) =>
        {
            head
        }
        _ => name,
    }
}

/// True for ports enumeration leaves out: Midi Through and this app's own ports. `id` is the
/// stripped name, whose client part comes first.
fn skipped_port(id: &str) -> bool {
    let client = id.split_once(':').map_or(id, |(client, _)| client);
    client == MIDI_THROUGH || client == CLIENT_NAME
}

fn device_error(error: impl std::fmt::Display) -> MidiError {
    MidiError::Device(error.to_string())
}

/// The ports of `io` as (info, port) pairs, skipped ports removed. A port that vanishes between
/// listing and naming is left out.
fn list<T: MidiIO>(io: &T) -> Vec<(MidiPortInfo, T::Port)> {
    io.ports()
        .into_iter()
        .filter_map(|port| {
            let full = io.port_name(&port).ok()?;
            let id = strip_address(&full);
            if skipped_port(id) {
                return None;
            }
            let id = id.to_owned();
            Some((
                MidiPortInfo {
                    name: id.clone(),
                    id,
                },
                port,
            ))
        })
        .collect()
}

fn not_available(id: &str) -> MidiError {
    MidiError::Device(format!("MIDI port {id} is not available"))
}

pub struct MidirBackend {
    snapshot: PortSnapshot,
}

impl MidirBackend {
    /// Connects to the host's MIDI system. Fails when it is not available (on Linux: no
    /// `/dev/snd/seq`).
    pub fn open() -> Result<Self, MidiError> {
        MidiInput::new(CLIENT_NAME).map_err(device_error)?;
        Ok(Self {
            snapshot: PortSnapshot::default(),
        })
    }

    fn input_client() -> Result<MidiInput, MidiError> {
        let mut input = MidiInput::new(CLIENT_NAME).map_err(device_error)?;
        // The mapper reads channel messages only.
        input.ignore(Ignore::All);
        Ok(input)
    }
}

impl MidiBackend for MidirBackend {
    fn name(&self) -> &'static str {
        "midir"
    }

    fn inputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError> {
        let client = Self::input_client()?;
        Ok(list(&client).into_iter().map(|(info, _)| info).collect())
    }

    fn outputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError> {
        let client = MidiOutput::new(CLIENT_NAME).map_err(device_error)?;
        Ok(list(&client).into_iter().map(|(info, _)| info).collect())
    }

    fn open_input(
        &mut self,
        id: &str,
        mut callback: MidiCallback,
    ) -> Result<Box<dyn MidiInputConnection>, MidiError> {
        let client = Self::input_client()?;
        let (_, port) = list(&client)
            .into_iter()
            .find(|(info, _)| info.id == id)
            .ok_or_else(|| not_available(id))?;
        let connection = client
            .connect(&port, "input", move |_stamp, bytes, ()| callback(bytes), ())
            .map_err(device_error)?;
        Ok(Box::new(MidirInput {
            _connection: connection,
        }))
    }

    fn open_output(&mut self, id: &str) -> Result<Box<dyn MidiOutputPort>, MidiError> {
        let client = MidiOutput::new(CLIENT_NAME).map_err(device_error)?;
        let (_, port) = list(&client)
            .into_iter()
            .find(|(info, _)| info.id == id)
            .ok_or_else(|| not_available(id))?;
        let connection = client.connect(&port, "output").map_err(device_error)?;
        Ok(Box::new(MidirOutput { connection }))
    }

    fn refresh(&mut self) -> Result<bool, MidiError> {
        let inputs = self.inputs()?;
        let outputs = self.outputs()?;
        Ok(self.snapshot.update(inputs, outputs))
    }
}

/// An open input; dropping it closes the midir connection and stops the callback.
struct MidirInput {
    _connection: midir::MidiInputConnection<()>,
}

impl MidiInputConnection for MidirInput {}

struct MidirOutput {
    connection: midir::MidiOutputConnection,
}

impl MidiOutputPort for MidirOutput {
    fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.connection.send(bytes).map_err(device_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trailing_sequencer_address_is_stripped() {
        assert_eq!(
            strip_address("Launchpad Mini:Launchpad Mini MIDI 1 24:0"),
            "Launchpad Mini:Launchpad Mini MIDI 1"
        );
        assert_eq!(
            strip_address("Midi Through:Midi Through Port-0 14:0"),
            "Midi Through:Midi Through Port-0"
        );
    }

    #[test]
    fn names_without_an_address_are_kept() {
        assert_eq!(strip_address("IAC Driver Bus 1"), "IAC Driver Bus 1");
        assert_eq!(strip_address("Port 1:2x"), "Port 1:2x");
        assert_eq!(strip_address("Pad 1: 2"), "Pad 1: 2");
        assert_eq!(strip_address("single"), "single");
        assert_eq!(strip_address(""), "");
    }

    #[test]
    fn midi_through_and_this_app_are_not_enumerated() {
        assert!(skipped_port("Midi Through:Midi Through Port-0"));
        assert!(skipped_port("openmixer:input"));
        assert!(!skipped_port("Launchpad Mini:Launchpad Mini MIDI 1"));
        assert!(!skipped_port("openmixer pad:Port"));
    }
}
