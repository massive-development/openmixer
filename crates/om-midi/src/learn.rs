//! MIDI learn: arm a target id, the next CC, note or pitch bend becomes a [`Mapping`] for it.
//! Arm and cancel come from the control thread, capture from the MIDI thread. The critical sections
//! are tiny and an atomic flag keeps the not-armed path lock-free. The mapper owns one and installs
//! the result.

use crate::event::{MidiEvent, MidiKind};
use crate::lock;
use crate::map::{MapMode, Mapping};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// What the learned control is. `Auto`: notes become buttons, CCs and pitch bend absolute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LearnType {
    Auto,
    Absolute,
    Relative,
    RelativeTwosComplement,
    Button,
    Toggle,
    Absolute14,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LearnRequest {
    pub target: String,
    pub learn_type: LearnType,
    pub min: f64,
    pub max: f64,
    /// Negative: the mapping default, `(max - min) / 127`.
    pub step: f64,
    pub initial: f64,
    /// `false`: bind to the channel the control sent on.
    pub any_channel: bool,
    pub invert: bool,
}

impl Default for LearnRequest {
    fn default() -> Self {
        Self {
            target: String::new(),
            learn_type: LearnType::Auto,
            min: 0.0,
            max: 1.0,
            step: -1.0,
            initial: 0.0,
            any_channel: false,
            invert: false,
        }
    }
}

impl LearnRequest {
    /// A new request for `target` with the default range and type.
    pub fn new(target: &str) -> Self {
        Self {
            target: target.to_owned(),
            ..Self::default()
        }
    }

    /// Sensible type and range for a router id (eq -48..6 dB, filter -1..1, rate 0.92..1.08, buttons
    /// for pfl/play/...).
    pub fn for_target(id: &str) -> Self {
        let mut r = Self::new(id);
        if ends_with_any(id, &[".eq.low", ".eq.mid", ".eq.high"]) {
            r.min = -48.0;
            r.max = 6.0;
            r.initial = 0.0;
        } else if id.ends_with(".filter") {
            r.min = -1.0;
            r.max = 1.0;
            r.initial = 0.0;
        } else if id.ends_with(".rate") {
            r.min = 0.92;
            r.max = 1.08;
            r.initial = 1.0;
        } else if id == "mixer.master" || id == "mixer.cue.level" {
            r.max = 2.0;
        } else if id.ends_with(".trim") {
            r.max = 4.0;
            r.initial = 1.0;
        }
        if ends_with_any(
            id,
            &[
                ".play",
                ".keylock",
                ".sync",
                ".sync.master",
                ".slip",
                ".quantize",
            ],
        ) {
            r.learn_type = LearnType::Toggle;
        } else if ends_with_any(id, &[".pfl", ".clear", ".loop.exit", ".loop.reloop"])
            || id.contains(".hotcue")
            || id.contains(".loop.quantized.")
        {
            r.learn_type = LearnType::Button;
        }
        r
    }
}

fn ends_with_any(id: &str, tails: &[&str]) -> bool {
    tails.iter().any(|t| id.ends_with(t))
}

#[derive(Debug, Default)]
pub struct MidiLearn {
    request: Mutex<LearnRequest>,
    armed: AtomicBool,
}

impl MidiLearn {
    pub fn new() -> Self {
        Self::default()
    }

    /// Arms with `request`. An empty target disarms.
    pub fn arm(&self, request: LearnRequest) {
        let mut guard = lock(&self.request);
        self.armed
            .store(!request.target.is_empty(), Ordering::Release);
        *guard = request;
    }

    pub fn cancel(&self) {
        let _guard = lock(&self.request);
        self.armed.store(false, Ordering::Release);
    }

    pub fn is_armed(&self) -> bool {
        self.armed.load(Ordering::Acquire)
    }

    pub fn request(&self) -> LearnRequest {
        lock(&self.request).clone()
    }

    /// MIDI thread: returns the new mapping and disarms, or nothing (not armed, or an event that
    /// cannot bind, such as a note-off). The event is consumed whenever a mapping is returned.
    pub fn capture(&self, e: &MidiEvent) -> Option<Mapping> {
        if !self.is_armed() || (e.kind == MidiKind::Note && !e.note_on) {
            return None;
        }
        let request = lock(&self.request);
        if !self.armed.load(Ordering::Relaxed) {
            return None;
        }
        let mut m = Mapping {
            kind: e.kind,
            channel: if request.any_channel { 0 } else { e.channel },
            number: e.number,
            target: request.target.clone(),
            mode: mode_for(request.learn_type, e.kind),
            min: request.min,
            max: request.max,
            initial: request.initial,
            invert: request.invert,
            step: if request.step >= 0.0 {
                request.step
            } else {
                (request.max - request.min) / 127.0
            },
            ..Mapping::default()
        };
        if m.mode == MapMode::Absolute14 && !resolve14(&mut m) {
            // Stays armed: the user can wiggle another control.
            return None;
        }
        self.armed.store(false, Ordering::Release);
        Some(m)
    }
}

fn mode_for(t: LearnType, kind: MidiKind) -> MapMode {
    match t {
        LearnType::Relative => MapMode::Relative,
        LearnType::RelativeTwosComplement => MapMode::RelativeTwosComplement,
        LearnType::Button => MapMode::Button,
        LearnType::Toggle => MapMode::Toggle,
        LearnType::Absolute14 => MapMode::Absolute14,
        LearnType::Absolute => MapMode::Absolute,
        LearnType::Auto => {
            if kind == MidiKind::Note {
                MapMode::Button
            } else {
                MapMode::Absolute
            }
        }
    }
}

/// Absolute14 binds the MSB (0-31) with its LSB 32 higher; a control that sent the LSB first is
/// accepted too.
fn resolve14(m: &mut Mapping) -> bool {
    if m.kind != MidiKind::Cc {
        return false;
    }
    if (32..64).contains(&m.number) {
        m.number -= 32;
    }
    if m.number >= 32 {
        return false;
    }
    m.lsb_number = m.number + 32;
    true
}
