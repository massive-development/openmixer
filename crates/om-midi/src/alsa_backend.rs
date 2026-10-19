//! ALSA sequencer backend (Linux). Built on the `alsa` crate that cpal already links, so the build
//! carries one `alsa`. Each routed direction is its own sequencer client, subscribed to the device's
//! port. An input runs a thread that polls its client and forwards each message's bytes; dropping
//! the connection stops and joins that thread.
//!
//! A port's id is its name (`Launchpad Mini MIDI 1`), the same string the user sees. A replugged
//! device keeps its name, so a saved preference still matches it.

use crate::backend::{
    MidiBackend, MidiCallback, MidiInputConnection, MidiOutputPort, MidiPortInfo, PortSnapshot,
};
use crate::error::MidiError;
use alsa::poll::poll;
use alsa::seq::{
    Addr, ClientIter, EvCtrl, EvNote, Event, EventType, PortCap, PortIter, PortSubscribe, PortType,
    Seq,
};
use alsa::{Direction, PollDescriptors};
use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// The sequencer client name for every client this app opens. Enumeration skips them.
const CLIENT_NAME: &str = "openmixer";
const CLIENT: &CStr = c"openmixer";
/// How long the input thread waits for events before it checks for a stop request.
const POLL_MS: i32 = 100;

pub struct AlsaBackend {
    snapshot: PortSnapshot,
}

/// A port's sequencer address, resolved from its id at open time.
#[derive(Clone, Copy)]
struct Endpoint {
    client: i32,
    port: i32,
}

fn device_error(error: alsa::Error) -> MidiError {
    MidiError::Device(error.to_string())
}

/// Opens a sequencer client on the default device and names it, so enumeration can skip it.
fn open_client(direction: Option<Direction>, nonblock: bool) -> Result<Seq, MidiError> {
    let seq = Seq::open(None, direction, nonblock).map_err(device_error)?;
    seq.set_client_name(CLIENT).map_err(device_error)?;
    Ok(seq)
}

impl AlsaBackend {
    /// Connects to the sequencer. Fails when it is not available (no `/dev/snd/seq`, no ALSA).
    pub fn open() -> Result<Self, MidiError> {
        open_client(None, false)?;
        Ok(Self {
            snapshot: PortSnapshot::default(),
        })
    }

    /// Every port with capability `want` (`SUBS_READ` for inputs, `SUBS_WRITE` for outputs), on
    /// clients other than the system and this app.
    fn ports(&self, want: PortCap) -> Result<Vec<(MidiPortInfo, Endpoint)>, MidiError> {
        let seq = open_client(None, false)?;
        let mut found = Vec::new();
        for client in ClientIter::new(&seq) {
            let client_id = client.get_client();
            let client_name = client.get_name().map_err(device_error)?.to_owned();
            if client_id == 0 || client_name == CLIENT_NAME {
                continue;
            }
            for port in PortIter::new(&seq, client_id) {
                if !port.get_capability().contains(want) {
                    continue;
                }
                let port_name = port.get_name().map_err(device_error)?.to_owned();
                found.push((
                    MidiPortInfo {
                        id: port_name.clone(),
                        name: port_name,
                    },
                    Endpoint {
                        client: client_id,
                        port: port.get_port(),
                    },
                ));
            }
        }
        Ok(found)
    }

    fn endpoint(&self, id: &str, want: PortCap) -> Result<Endpoint, MidiError> {
        self.ports(want)?
            .into_iter()
            .find(|(info, _)| info.id == id)
            .map(|(_, endpoint)| endpoint)
            .ok_or_else(|| MidiError::Device(format!("MIDI port {id} is not available")))
    }
}

impl MidiBackend for AlsaBackend {
    fn name(&self) -> &'static str {
        "alsa"
    }

    fn inputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError> {
        Ok(self
            .ports(PortCap::SUBS_READ)?
            .into_iter()
            .map(|(p, _)| p)
            .collect())
    }

    fn outputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError> {
        Ok(self
            .ports(PortCap::SUBS_WRITE)?
            .into_iter()
            .map(|(p, _)| p)
            .collect())
    }

    fn open_input(
        &mut self,
        id: &str,
        callback: MidiCallback,
    ) -> Result<Box<dyn MidiInputConnection>, MidiError> {
        let device = self.endpoint(id, PortCap::SUBS_READ)?;
        let seq = open_client(Some(Direction::Capture), true)?;
        let port = seq
            .create_simple_port(
                c"input",
                PortCap::WRITE | PortCap::SUBS_WRITE,
                PortType::MIDI_GENERIC | PortType::APPLICATION,
            )
            .map_err(device_error)?;
        let subscription = PortSubscribe::empty().map_err(device_error)?;
        subscription.set_sender(Addr {
            client: device.client,
            port: device.port,
        });
        subscription.set_dest(Addr {
            client: seq.client_id().map_err(device_error)?,
            port,
        });
        seq.subscribe_port(&subscription).map_err(device_error)?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("openmixer-midi-in".to_owned())
            .spawn(move || run_input(seq, stop_flag, callback))
            .map_err(MidiError::Io)?;
        Ok(Box::new(AlsaInput {
            stop,
            thread: Some(thread),
        }))
    }

    fn open_output(&mut self, id: &str) -> Result<Box<dyn MidiOutputPort>, MidiError> {
        let device = self.endpoint(id, PortCap::SUBS_WRITE)?;
        let seq = open_client(Some(Direction::Playback), false)?;
        let port = seq
            .create_simple_port(
                c"output",
                PortCap::READ | PortCap::SUBS_READ,
                PortType::MIDI_GENERIC | PortType::APPLICATION,
            )
            .map_err(device_error)?;
        let subscription = PortSubscribe::empty().map_err(device_error)?;
        subscription.set_sender(Addr {
            client: seq.client_id().map_err(device_error)?,
            port,
        });
        subscription.set_dest(Addr {
            client: device.client,
            port: device.port,
        });
        seq.subscribe_port(&subscription).map_err(device_error)?;
        Ok(Box::new(AlsaOutput { seq, port }))
    }

    fn refresh(&mut self) -> Result<bool, MidiError> {
        let inputs = self.inputs()?;
        let outputs = self.outputs()?;
        Ok(self.snapshot.update(inputs, outputs))
    }
}

/// Input thread body: poll for events, forward each complete message, repeat until stopped.
fn run_input(seq: Seq, stop: Arc<AtomicBool>, mut callback: MidiCallback) {
    let mut input = seq.input();
    while !stop.load(Ordering::Acquire) {
        if let Ok(mut fds) = (&seq, Some(Direction::Capture)).get() {
            let _ = poll(&mut fds, POLL_MS);
        }
        while input.event_input_pending(true).unwrap_or(0) > 0 {
            let Ok(event) = input.event_input() else {
                break;
            };
            if let Some((bytes, len)) = to_bytes(&event) {
                callback(&bytes[..len]);
            }
        }
    }
}

/// The raw bytes of a sequencer event that the mapper reads (channel messages), status first.
fn to_bytes(event: &Event) -> Option<([u8; 3], usize)> {
    match event.get_type() {
        EventType::Noteon | EventType::Noteoff => {
            let note: EvNote = event.get_data()?;
            let status = if event.get_type() == EventType::Noteon {
                0x90
            } else {
                0x80
            };
            Some((
                [status | (note.channel & 0x0F), note.note, note.velocity],
                3,
            ))
        }
        EventType::Controller => {
            let ctrl: EvCtrl = event.get_data()?;
            Some((
                [
                    0xB0 | (ctrl.channel & 0x0F),
                    ctrl.param as u8,
                    ctrl.value as u8,
                ],
                3,
            ))
        }
        EventType::Pitchbend => {
            let ctrl: EvCtrl = event.get_data()?;
            let value = (ctrl.value + 8192).clamp(0, 16383) as u16;
            Some((
                [
                    0xE0 | (ctrl.channel & 0x0F),
                    (value & 0x7F) as u8,
                    (value >> 7) as u8,
                ],
                3,
            ))
        }
        _ => None,
    }
}

/// The sequencer event for an outgoing channel message, or `None` for a type the app does not send.
fn to_event(bytes: &[u8]) -> Option<Event<'static>> {
    let status = *bytes.first()?;
    if bytes.len() < 3 {
        return None;
    }
    let channel = status & 0x0F;
    let (d1, d2) = (bytes[1], bytes[2]);
    match status & 0xF0 {
        0x80 => Some(Event::new(
            EventType::Noteoff,
            &EvNote {
                channel,
                note: d1,
                velocity: d2,
                off_velocity: 0,
                duration: 0,
            },
        )),
        0x90 => Some(Event::new(
            EventType::Noteon,
            &EvNote {
                channel,
                note: d1,
                velocity: d2,
                off_velocity: 0,
                duration: 0,
            },
        )),
        0xB0 => Some(Event::new(
            EventType::Controller,
            &EvCtrl {
                channel,
                param: u32::from(d1),
                value: i32::from(d2),
            },
        )),
        0xE0 => {
            let value = i32::from(d1) | (i32::from(d2) << 7);
            Some(Event::new(
                EventType::Pitchbend,
                &EvCtrl {
                    channel,
                    param: 0,
                    value: value - 8192,
                },
            ))
        }
        _ => None,
    }
}

struct AlsaInput {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MidiInputConnection for AlsaInput {}

impl Drop for AlsaInput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct AlsaOutput {
    seq: Seq,
    port: i32,
}

impl MidiOutputPort for AlsaOutput {
    fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
        let Some(mut event) = to_event(bytes) else {
            return Ok(());
        };
        event.set_source(self.port);
        event.set_subs();
        event.set_direct();
        self.seq
            .event_output_direct(&mut event)
            .map_err(device_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_and_pitch_bend_round_trip_through_sequencer_events() {
        let cc = to_event(&[0xB2, 7, 100]).expect("cc");
        assert_eq!(cc.get_type(), EventType::Controller);
        let ctrl: EvCtrl = cc.get_data().expect("data");
        assert_eq!((ctrl.channel, ctrl.param, ctrl.value), (2, 7, 100));

        let bend = to_event(&[0xE0, 0x00, 0x40]).expect("bend");
        let (bytes, len) = to_bytes(&bend).expect("bytes");
        assert_eq!(&bytes[..len], &[0xE0, 0x00, 0x40]);
    }

    #[test]
    fn unsupported_messages_are_not_sent() {
        assert!(to_event(&[0xC0, 5, 0]).is_none(), "program change");
        assert!(to_event(&[0xB0]).is_none(), "truncated");
    }
}
