//! Applies a [`MidiMap`] to incoming events and forwards the results to a [`ParameterTarget`]. Also
//! the editing surface for the map: learn mode, binding CRUD and save.
//!
//! Control-thread safe: a map reload or edit is atomic with respect to `handle_event` on the MIDI
//! thread. The state lock is held across the target call, so the target must not call back into
//! this mapper on the same thread. Never call from the audio thread.

use crate::error::MidiError;
use crate::event::{MidiEvent, MidiKind};
use crate::learn::{LearnRequest, MidiLearn};
use crate::lock;
use crate::map::{MapMode, Mapping, MidiMap};
use crate::output_rule::OutputRule;
use om_core::{ParameterTarget, StateReader};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type LearnCallback = Arc<dyn Fn(&Mapping) + Send + Sync>;
type ChannelKey = (MidiKind, i32, i32);

#[derive(Default)]
struct State {
    target: Option<Arc<dyn ParameterTarget>>,
    map: MidiMap,
    /// (kind, channel, number) to the mappings that listen there; channel 0 is "any".
    index: HashMap<ChannelKey, Vec<usize>>,
    /// Last value produced per parameter id, or its initial value.
    values: HashMap<String, f64>,
    /// Per (channel, MSB controller): the 14-bit value so far.
    halves14: HashMap<(i32, i32), i32>,
    on_learned: Option<LearnCallback>,
    /// Reads the engine's live values. A toggle or relative message starts from the engine's value
    /// when this is installed, so the controller agrees with the UI and the engine.
    reader: Option<Arc<dyn StateReader>>,
}

#[derive(Default)]
pub struct MidiMapper {
    state: Mutex<State>,
    learn: MidiLearn,
    generation: AtomicU64,
}

impl MidiMapper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where produced parameter updates go.
    pub fn set_target(&self, target: Arc<dyn ParameterTarget>) {
        lock(&self.state).target = Some(target);
    }

    /// Where the engine's live values are read from (see [`State::reader`]).
    pub fn set_state_reader(&self, reader: Arc<dyn StateReader>) {
        lock(&self.state).reader = Some(reader);
    }

    /// Replaces the map. A bad map changes nothing.
    pub fn load_from_json(&self, json: &str) -> Result<(), MidiError> {
        let parsed = MidiMap::parse(json)?;
        self.install(parsed);
        Ok(())
    }

    pub fn load_from_file(&self, path: &Path) -> Result<(), MidiError> {
        if !path.is_file() {
            return Err(MidiError::NotFound(path.to_path_buf()));
        }
        let text = fs::read_to_string(path)?;
        self.load_from_json(&text)
    }

    /// Writes the current map (schema v2) to `path`, creating parent folders. The write replaces
    /// the file through a temporary sibling, so a crash mid-save keeps the old map.
    pub fn save_to_file(&self, path: &Path) -> Result<(), MidiError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        fs::write(&tmp, self.to_json())?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn to_json(&self) -> String {
        lock(&self.state).map.to_json()
    }

    pub fn name(&self) -> String {
        lock(&self.state).map.name.clone()
    }

    pub fn set_name(&self, name: &str) {
        lock(&self.state).map.name = name.to_owned();
        self.bump();
    }

    pub fn num_mappings(&self) -> usize {
        lock(&self.state).map.mappings.len()
    }

    /// Feeds one raw message through the map. Returns the number of parameter updates produced.
    pub fn handle_message(&self, bytes: &[u8]) -> usize {
        MidiEvent::from_bytes(bytes).map_or(0, |e| self.handle_event(&e))
    }

    /// Feeds one normalised event through the map. While learn is armed the event is captured
    /// instead (and not forwarded).
    pub fn handle_event(&self, e: &MidiEvent) -> usize {
        if self.learn.is_armed() && self.capture_learned(e) {
            return 0;
        }
        let mut guard = lock(&self.state);
        let State {
            map,
            index,
            values,
            halves14,
            target,
            reader,
            ..
        } = &mut *guard;
        let mut fired = 0;
        for channel in [e.channel, 0] {
            let Some(listeners) = index.get(&(e.kind, channel, e.number)) else {
                continue;
            };
            for &i in listeners {
                let source = Source {
                    values,
                    halves14,
                    target: target.as_deref(),
                    reader: reader.as_deref(),
                };
                if apply(&map.mappings[i], e, source) {
                    fired += 1;
                }
            }
        }
        fired
    }

    /// Last value produced for a target, or 0 when it has never been set.
    pub fn value(&self, target_id: &str) -> f64 {
        lock(&self.state)
            .values
            .get(target_id)
            .copied()
            .unwrap_or(0.0)
    }

    /// Snapshot of the bindings; indices refer to this order.
    pub fn bindings(&self) -> Vec<Mapping> {
        lock(&self.state).map.mappings.clone()
    }

    /// Appends a binding. Fails, leaving the map unchanged, for an invalid mapping.
    pub fn add_binding(&self, m: Mapping) -> Result<(), MidiError> {
        m.validate()?;
        {
            let mut s = lock(&self.state);
            s.map.mappings.push(m);
            rebuild_index(&mut s);
        }
        self.bump();
        Ok(())
    }

    pub fn replace_binding(&self, index: usize, m: Mapping) -> Result<(), MidiError> {
        m.validate()?;
        {
            let mut s = lock(&self.state);
            let slot = s
                .map
                .mappings
                .get_mut(index)
                .ok_or_else(|| MidiError::Invalid(format!("no binding at index {index}")))?;
            *slot = m;
            rebuild_index(&mut s);
        }
        self.bump();
        Ok(())
    }

    /// Removes the binding at `index`; false when there is none.
    pub fn remove_binding(&self, index: usize) -> bool {
        let removed = {
            let mut s = lock(&self.state);
            if index >= s.map.mappings.len() {
                false
            } else {
                s.map.mappings.remove(index);
                rebuild_index(&mut s);
                true
            }
        };
        if removed {
            self.bump();
        }
        removed
    }

    /// Removes every binding for `target_id`; returns how many were removed.
    pub fn remove_bindings_for(&self, target_id: &str) -> usize {
        let removed = {
            let mut s = lock(&self.state);
            let before = s.map.mappings.len();
            s.map.mappings.retain(|m| m.target != target_id);
            let removed = before - s.map.mappings.len();
            rebuild_index(&mut s);
            removed
        };
        self.bump();
        removed
    }

    /// Feedback rules, in map order.
    pub fn outputs(&self) -> Vec<OutputRule> {
        lock(&self.state).map.outputs.clone()
    }

    pub fn set_outputs(&self, rules: Vec<OutputRule>) {
        lock(&self.state).map.outputs = rules;
        self.bump();
    }

    /// The next CC, note or pitch bend binds to `request.target` and is not forwarded.
    pub fn arm_learn(&self, request: LearnRequest) {
        self.learn.arm(request);
    }

    pub fn cancel_learn(&self) {
        self.learn.cancel();
    }

    pub fn is_learning(&self) -> bool {
        self.learn.is_armed()
    }

    /// Runs on the MIDI thread, outside the state lock: post to the UI thread before touching UI.
    pub fn set_on_learned<F>(&self, callback: F)
    where
        F: Fn(&Mapping) + Send + Sync + 'static,
    {
        lock(&self.state).on_learned = Some(Arc::new(callback));
    }

    /// Bumped by every load or edit; the feedback unit reloads its rules when it changes.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Replaces the map. The last value of every target the new map still has is kept, so a
    /// relative control does not jump back to its initial value on a reload.
    fn install(&self, new_map: MidiMap) {
        {
            let mut s = lock(&self.state);
            let targets: HashSet<String> =
                new_map.mappings.iter().map(|m| m.target.clone()).collect();
            s.values.retain(|id, _| targets.contains(id));
            s.map = new_map;
            s.halves14.clear();
            rebuild_index(&mut s);
        }
        self.bump();
    }

    /// MIDI thread: a learned control replaces whatever the target or the control was bound to.
    fn capture_learned(&self, e: &MidiEvent) -> bool {
        let Some(learned) = self.learn.capture(e) else {
            return false;
        };
        let callback = {
            let mut s = lock(&self.state);
            bind(&mut s, learned.clone());
            s.on_learned.clone()
        };
        self.bump();
        if let Some(callback) = callback {
            callback(&learned);
        }
        true
    }
}

fn rebuild_index(s: &mut State) {
    s.index.clear();
    for (i, m) in s.map.mappings.iter().enumerate() {
        s.index
            .entry((m.kind, m.channel, m.number))
            .or_default()
            .push(i);
        if m.mode == MapMode::Absolute14 {
            s.index
                .entry((m.kind, m.channel, m.lsb_number))
                .or_default()
                .push(i);
        }
        s.values.entry(m.target.clone()).or_insert(m.initial);
    }
}

fn bind(s: &mut State, learned: Mapping) {
    s.map.mappings.retain(|m| {
        !(m.target == learned.target
            || (m.kind == learned.kind
                && m.channel == learned.channel
                && m.number == learned.number))
    });
    s.values.insert(learned.target.clone(), learned.initial);
    s.map.mappings.push(learned);
    rebuild_index(s);
}

/// The mutable pieces a mapping application reads and writes.
struct Source<'a> {
    values: &'a mut HashMap<String, f64>,
    halves14: &'a mut HashMap<(i32, i32), i32>,
    target: Option<&'a dyn ParameterTarget>,
    reader: Option<&'a dyn StateReader>,
}

/// Applies one mapping to an event. Returns true when a parameter update was produced.
fn apply(m: &Mapping, incoming: &MidiEvent, source: Source<'_>) -> bool {
    let Source {
        values,
        halves14,
        target,
        reader,
    } = source;
    let mut e = *incoming;
    if m.mode == MapMode::Absolute14 {
        combine14(halves14, m, &mut e);
    }
    let stored = values.get(m.target.as_str()).copied().unwrap_or(0.0);
    // Toggle and relative work from the current value; the engine's is the truth when it is known.
    let current = match m.mode {
        MapMode::Toggle | MapMode::Relative | MapMode::RelativeTwosComplement => reader
            .and_then(|r| r.read_state(&m.target))
            .unwrap_or(stored),
        _ => stored,
    };
    let Some(raw) = compute(m, &e, current) else {
        return false;
    };
    let value = raw.clamp(m.min.min(m.max), m.min.max(m.max));
    match values.get_mut(m.target.as_str()) {
        Some(slot) => *slot = value,
        None => {
            values.insert(m.target.clone(), value);
        }
    }
    if let Some(target) = target {
        target.set_parameter(&m.target, value);
    }
    true
}

/// The new value for `m` given `e`, or `None` when the event does not change the parameter
/// (a toggle's release, for one).
fn compute(m: &Mapping, e: &MidiEvent, current: f64) -> Option<f64> {
    let pressed = if e.kind == MidiKind::Note {
        e.note_on
    } else {
        e.value > 0
    };
    match m.mode {
        MapMode::Absolute | MapMode::Absolute14 => {
            let wide = e.kind == MidiKind::PitchBend || m.mode == MapMode::Absolute14;
            let norm = f64::from(e.value) / if wide { 16383.0 } else { 127.0 };
            let norm = if m.invert { 1.0 - norm } else { norm };
            Some(m.min + norm * (m.max - m.min))
        }
        MapMode::Relative | MapMode::RelativeTwosComplement => {
            let raw = if m.mode == MapMode::Relative {
                e.value - 64
            } else if e.value < 64 {
                e.value
            } else {
                e.value - 128
            };
            let raw = if m.invert { -raw } else { raw };
            Some(current + f64::from(raw) * m.step)
        }
        MapMode::Button => Some(if pressed != m.invert { m.max } else { m.min }),
        MapMode::Toggle => pressed.then_some(if current == m.max { m.min } else { m.max }),
    }
}

/// MSB and LSB arrive as separate CCs; keep the halves and report the combined 14-bit value.
fn combine14(halves14: &mut HashMap<(i32, i32), i32>, m: &Mapping, e: &mut MidiEvent) {
    let bits = halves14.entry((e.channel, m.number)).or_insert(0);
    *bits = if e.number == m.number {
        (e.value << 7) | (*bits & 0x7F)
    } else {
        (*bits & !0x7F) | e.value
    };
    e.value = *bits;
}
