//! In-memory [`MidiBackend`] for tests and headless runs: a fixed device list, injected input bytes
//! and recorded output bytes. It never touches the host's MIDI system.

use crate::backend::{
    MidiBackend, MidiCallback, MidiInputConnection, MidiOutputPort, MidiPortInfo, PortSnapshot,
};
use crate::error::MidiError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Devices {
    inputs: Vec<MidiPortInfo>,
    outputs: Vec<MidiPortInfo>,
    callbacks: HashMap<String, Arc<Mutex<MidiCallback>>>,
    sent: HashMap<String, Vec<Vec<u8>>>,
}

/// Clones share one device set, so a test keeps a handle after giving a copy to a router.
#[derive(Clone, Default)]
pub struct MockBackend {
    devices: Arc<Mutex<Devices>>,
    snapshot: Arc<Mutex<PortSnapshot>>,
}

fn port(id: &str) -> MidiPortInfo {
    MidiPortInfo {
        id: id.to_owned(),
        name: id.to_owned(),
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an input and an output with the same name, as a controller usually shows up.
    pub fn add_device(&self, name: &str) {
        let mut d = self.lock();
        d.inputs.push(port(name));
        d.outputs.push(port(name));
    }

    pub fn add_input(&self, name: &str) {
        self.lock().inputs.push(port(name));
    }

    pub fn add_output(&self, name: &str) {
        self.lock().outputs.push(port(name));
    }

    /// Removes the device's ports, as an unplug does. An open input stops receiving.
    pub fn remove_device(&self, name: &str) {
        let mut d = self.lock();
        d.inputs.retain(|p| p.id != name);
        d.outputs.retain(|p| p.id != name);
        d.callbacks.remove(name);
    }

    /// Delivers `bytes` to the open input `id`, as the device would. False when it is not open.
    pub fn inject(&self, id: &str, bytes: &[u8]) -> bool {
        let callback = self.lock().callbacks.get(id).cloned();
        match callback {
            Some(cb) => {
                (cb.lock().unwrap_or_else(std::sync::PoisonError::into_inner))(bytes);
                true
            }
            None => false,
        }
    }

    /// Every message sent to the output `id`, in order.
    pub fn sent(&self, id: &str) -> Vec<Vec<u8>> {
        self.lock().sent.get(id).cloned().unwrap_or_default()
    }

    /// Whether an input is currently open.
    pub fn is_open(&self, id: &str) -> bool {
        self.lock().callbacks.contains_key(id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Devices> {
        self.devices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct MockInput {
    id: String,
    devices: Arc<Mutex<Devices>>,
}

impl MidiInputConnection for MockInput {}

impl Drop for MockInput {
    fn drop(&mut self) {
        let mut d = self
            .devices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        d.callbacks.remove(&self.id);
    }
}

struct MockOutput {
    id: String,
    devices: Arc<Mutex<Devices>>,
}

impl MidiOutputPort for MockOutput {
    fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
        let mut d = self
            .devices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        d.sent
            .entry(self.id.clone())
            .or_default()
            .push(bytes.to_vec());
        Ok(())
    }
}

impl MidiBackend for MockBackend {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn inputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError> {
        Ok(self.lock().inputs.clone())
    }

    fn outputs(&mut self) -> Result<Vec<MidiPortInfo>, MidiError> {
        Ok(self.lock().outputs.clone())
    }

    fn open_input(
        &mut self,
        id: &str,
        callback: MidiCallback,
    ) -> Result<Box<dyn MidiInputConnection>, MidiError> {
        let mut d = self.lock();
        if !d.inputs.iter().any(|p| p.id == id) {
            return Err(MidiError::Device(format!("no input named {id}")));
        }
        if d.callbacks.contains_key(id) {
            return Err(MidiError::Device(format!("input {id} is already open")));
        }
        d.callbacks
            .insert(id.to_owned(), Arc::new(Mutex::new(callback)));
        Ok(Box::new(MockInput {
            id: id.to_owned(),
            devices: Arc::clone(&self.devices),
        }))
    }

    fn open_output(&mut self, id: &str) -> Result<Box<dyn MidiOutputPort>, MidiError> {
        if !self.lock().outputs.iter().any(|p| p.id == id) {
            return Err(MidiError::Device(format!("no output named {id}")));
        }
        Ok(Box::new(MockOutput {
            id: id.to_owned(),
            devices: Arc::clone(&self.devices),
        }))
    }

    fn refresh(&mut self) -> Result<bool, MidiError> {
        let (inputs, outputs) = {
            let d = self.lock();
            (d.inputs.clone(), d.outputs.clone())
        };
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(snapshot.update(inputs, outputs))
    }
}
