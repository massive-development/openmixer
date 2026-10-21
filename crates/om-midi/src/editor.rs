//! The mapping editor's model: edits to the live map with validation, undo and redo, conflict
//! detection, MIDI learn into an existing or a new binding, import and export, and a saved-state
//! check for the unsaved-changes guard.
//!
//! Every edit goes straight to the [`MidiMapper`] (live apply). Nothing is written to disk until
//! [`MapEditor::save`]. Undo keeps whole-map snapshots, bounded by [`HISTORY_LIMIT`].
//!
//! Control-thread only. Learn captures happen on the MIDI thread; the editor settles them when
//! the UI calls [`MapEditor::poll_learn`] or before the next edit.

use crate::error::MidiError;
use crate::event::MidiKind;
use crate::learn::{LearnRequest, LearnType};
use crate::map::{MapMode, Mapping, MidiMap};
use crate::mapper::MidiMapper;
use crate::output_rule::OutputRule;
use om_core::ParamId;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

/// Undo steps kept. Each step is a whole-map copy, so the bound keeps memory small.
pub const HISTORY_LIMIT: usize = 100;

/// Largest map file import reads. Real maps are a few KB; this only stops a hostile file.
pub const MAX_MAP_BYTES: u64 = 1024 * 1024;

/// Two bindings that would both fire on one incoming control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// Index of the earlier binding.
    pub first: usize,
    /// Index of the later binding.
    pub second: usize,
    /// The control both listen to, e.g. "Note 36 (any channel)".
    pub control: String,
}

/// Which side of a [`Conflict`] survives [`MapEditor::resolve_conflict`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    First,
    Second,
}

/// The form fields of one binding, as text, so a UI can show what the user typed and the error for
/// each field. [`BindingDraft::to_mapping`] is the single place that validates them.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingDraft {
    pub kind: MidiKind,
    /// "0" for any channel, else 1-16.
    pub channel: String,
    pub number: String,
    pub target: String,
    pub mode: MapMode,
    pub min: String,
    pub max: String,
    pub step: String,
    pub initial: String,
    pub invert: bool,
    /// Absolute14 only: the LSB controller.
    pub lsb: String,
}

impl BindingDraft {
    pub fn from_mapping(m: &Mapping) -> Self {
        Self {
            kind: m.kind,
            channel: m.channel.to_string(),
            number: m.number.to_string(),
            target: m.target.clone(),
            mode: m.mode,
            min: m.min.to_string(),
            max: m.max.to_string(),
            step: m.step.to_string(),
            initial: m.initial.to_string(),
            invert: m.invert,
            lsb: if m.lsb_number >= 0 {
                m.lsb_number.to_string()
            } else {
                String::new()
            },
        }
    }

    /// Parses and validates the fields. The error names the field that is wrong.
    pub fn to_mapping(&self) -> Result<Mapping, MidiError> {
        let target = self.target.trim();
        if target.is_empty() {
            return Err(invalid(
                "Target is empty: pick a parameter id such as deck.0.play",
            ));
        }
        ParamId::from_str(target)
            .map_err(|e| invalid(format!("Target \"{target}\" is not a parameter id ({e})")))?;
        let channel = int(&self.channel, "Channel")?;
        if !(0..=16).contains(&channel) {
            return Err(invalid("Channel must be 0 (any) or 1-16"));
        }
        let number = if self.kind == MidiKind::PitchBend {
            0
        } else {
            int(&self.number, "Number")?
        };
        if !(0..=127).contains(&number) {
            return Err(invalid("Number must be 0-127"));
        }
        let lsb_number = if self.mode == MapMode::Absolute14 {
            let lsb = int(&self.lsb, "LSB")?;
            if !(0..=127).contains(&lsb) {
                return Err(invalid("LSB must be 0-127"));
            }
            lsb
        } else {
            -1
        };
        let min = float(&self.min, "Min")?;
        let max = float(&self.max, "Max")?;
        if min == max {
            return Err(invalid("Min and max must differ"));
        }
        let step = if self.step.trim().is_empty() {
            (max - min) / 127.0
        } else {
            float(&self.step, "Step")?
        };
        let initial = if self.initial.trim().is_empty() {
            min
        } else {
            float(&self.initial, "Initial")?
        };
        let mapping = Mapping {
            kind: self.kind,
            channel,
            number,
            target: target.to_owned(),
            mode: self.mode,
            min,
            max,
            step,
            initial,
            invert: self.invert,
            lsb_number,
        };
        mapping.validate()?;
        Ok(mapping)
    }
}

fn int(text: &str, field: &str) -> Result<i32, MidiError> {
    text.trim()
        .parse::<i32>()
        .map_err(|_| invalid(format!("{field} must be a whole number")))
}

fn float(text: &str, field: &str) -> Result<f64, MidiError> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| invalid(format!("{field} must be a number")))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid(format!("{field} must be a finite number")))
    }
}

fn invalid(message: impl Into<String>) -> MidiError {
    MidiError::Invalid(message.into())
}

/// A readable name for a control, e.g. "Note 36 (any channel)" or "CC 7 on ch 1".
pub fn control_label(kind: MidiKind, channel: i32, number: i32) -> String {
    let name = match kind {
        MidiKind::Cc => format!("CC {number}"),
        MidiKind::Note => format!("Note {number}"),
        MidiKind::PitchBend => "Pitch bend".to_owned(),
    };
    if channel == 0 {
        format!("{name} (any channel)")
    } else {
        format!("{name} on ch {channel}")
    }
}

/// The control numbers a binding listens to: its own, and the LSB of an absolute14 pair.
fn addresses(m: &Mapping) -> Vec<i32> {
    let mut out = vec![m.number];
    if m.mode == MapMode::Absolute14 && m.lsb_number >= 0 {
        out.push(m.lsb_number);
    }
    out
}

fn channels_overlap(a: i32, b: i32) -> bool {
    a == 0 || b == 0 || a == b
}

/// Every pair of bindings that would both fire on one incoming message. A channel-0 (any) binding
/// overlaps every channel; two channel-specific bindings overlap only on the same channel.
pub fn find_conflicts(bindings: &[Mapping]) -> Vec<Conflict> {
    let mut out = Vec::new();
    for (i, a) in bindings.iter().enumerate() {
        for (j, b) in bindings.iter().enumerate().skip(i + 1) {
            if a.kind != b.kind || !channels_overlap(a.channel, b.channel) {
                continue;
            }
            let shared = addresses(a).into_iter().find(|n| addresses(b).contains(n));
            if let Some(number) = shared {
                let channel = if a.channel == 0 { b.channel } else { a.channel };
                out.push(Conflict {
                    first: i,
                    second: j,
                    control: control_label(a.kind, channel, number),
                });
            }
        }
    }
    out
}

/// The learn type that reproduces a binding's mode.
fn learn_type_for(mode: MapMode) -> LearnType {
    match mode {
        MapMode::Absolute => LearnType::Absolute,
        MapMode::Relative => LearnType::Relative,
        MapMode::RelativeTwosComplement => LearnType::RelativeTwosComplement,
        MapMode::Button => LearnType::Button,
        MapMode::Toggle => LearnType::Toggle,
        MapMode::Absolute14 => LearnType::Absolute14,
    }
}

/// Whether `target` is a parameter id the router knows the shape of.
fn check_target(target: &str) -> Result<(), MidiError> {
    ParamId::from_str(target)
        .map(|_| ())
        .map_err(|e| invalid(format!("Target \"{target}\" is not a parameter id ({e})")))
}

/// The editing model over the live map. See the module docs.
pub struct MapEditor {
    mapper: Arc<MidiMapper>,
    path: Option<PathBuf>,
    /// The serialised map as last saved or loaded; dirty means the live map differs from it.
    saved_json: String,
    undo: VecDeque<MidiMap>,
    redo: Vec<MidiMap>,
    /// The map before a learn that is still waiting for a control.
    pending_learn: Option<MidiMap>,
}

impl MapEditor {
    /// Edits `mapper`'s live map. `path` is the map file [`MapEditor::save`] writes; the live map
    /// counts as saved when the editor is made.
    pub fn new(mapper: Arc<MidiMapper>, path: Option<PathBuf>) -> Self {
        let saved_json = mapper.to_json();
        Self {
            mapper,
            path,
            saved_json,
            undo: VecDeque::new(),
            redo: Vec::new(),
            pending_learn: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    pub fn mapper(&self) -> &Arc<MidiMapper> {
        &self.mapper
    }

    pub fn bindings(&self) -> Vec<Mapping> {
        self.mapper.bindings()
    }

    pub fn outputs(&self) -> Vec<OutputRule> {
        self.mapper.outputs()
    }

    pub fn name(&self) -> String {
        self.mapper.snapshot().name
    }

    pub fn device(&self) -> String {
        self.mapper.snapshot().device
    }

    /// True when the live map differs from the last saved or loaded one.
    pub fn is_dirty(&self) -> bool {
        self.mapper.to_json() != self.saved_json
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending_learn.is_some()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn conflicts(&self) -> Vec<Conflict> {
        find_conflicts(&self.mapper.bindings())
    }

    /// Records the pre-change map, clears redo and bounds the history.
    fn record(&mut self, before: MidiMap) {
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(before);
        self.redo.clear();
    }

    /// Runs `edit` on the mapper and records an undo step when it succeeds. `edit` validates before
    /// it mutates, so a failure leaves the map as it was.
    fn change<T>(
        &mut self,
        edit: impl FnOnce(&MidiMapper) -> Result<T, MidiError>,
    ) -> Result<T, MidiError> {
        self.settle_learn();
        let before = self.mapper.snapshot();
        let value = edit(&self.mapper)?;
        self.record(before);
        Ok(value)
    }

    /// Adds a binding after validating it and its target. Returns its index.
    pub fn add_binding(&mut self, mapping: Mapping) -> Result<usize, MidiError> {
        check_target(&mapping.target)?;
        self.change(|m| {
            let index = m.bindings().len();
            m.add_binding(mapping)?;
            Ok(index)
        })
    }

    /// Replaces the binding at `index` with a validated one.
    pub fn set_binding(&mut self, index: usize, mapping: Mapping) -> Result<(), MidiError> {
        check_target(&mapping.target)?;
        self.change(|m| m.replace_binding(index, mapping))
    }

    /// Validates the form and applies it to the binding at `index`.
    pub fn apply_draft(&mut self, index: usize, draft: &BindingDraft) -> Result<(), MidiError> {
        let mapping = draft.to_mapping()?;
        self.set_binding(index, mapping)
    }

    /// Validates the form and adds it as a new binding.
    pub fn add_draft(&mut self, draft: &BindingDraft) -> Result<usize, MidiError> {
        let mapping = draft.to_mapping()?;
        self.add_binding(mapping)
    }

    /// Removes the binding at `index`.
    pub fn delete_binding(&mut self, index: usize) -> Result<(), MidiError> {
        self.change(|m| {
            if m.remove_binding(index) {
                Ok(())
            } else {
                Err(invalid(format!("No binding at index {index}")))
            }
        })
    }

    /// Adds a copy of the binding at `index` straight after it. Returns the copy's index.
    pub fn duplicate_binding(&mut self, index: usize) -> Result<usize, MidiError> {
        let original = self
            .mapper
            .bindings()
            .get(index)
            .cloned()
            .ok_or_else(|| invalid(format!("No binding at index {index}")))?;
        self.change(|m| {
            m.add_binding(original)?;
            Ok(m.bindings().len() - 1)
        })
    }

    /// Changes only the channel (0 = any) of the binding at `index`.
    pub fn set_channel(&mut self, index: usize, channel: i32) -> Result<(), MidiError> {
        let mut mapping = self
            .mapper
            .bindings()
            .get(index)
            .cloned()
            .ok_or_else(|| invalid(format!("No binding at index {index}")))?;
        mapping.channel = channel;
        self.set_binding(index, mapping)
    }

    /// Removes one side of a conflict. `conflict` must still be a current conflict.
    pub fn resolve_conflict(&mut self, conflict: &Conflict, keep: Keep) -> Result<(), MidiError> {
        self.settle_learn();
        let current = self.conflicts();
        if !current
            .iter()
            .any(|c| c.first == conflict.first && c.second == conflict.second)
        {
            return Err(invalid("That conflict is no longer present"));
        }
        let drop = match keep {
            Keep::First => conflict.second,
            Keep::Second => conflict.first,
        };
        self.delete_binding(drop)
    }

    /// Re-learns the input side of the binding at `index`. The next control that moves binds to
    /// the same target with the same range, step, initial value and invert; the old control is
    /// replaced. Channel-specific bindings learn on the channel the control sends on.
    pub fn learn_into(&mut self, index: usize) -> Result<(), MidiError> {
        self.settle_learn();
        let current = self
            .mapper
            .bindings()
            .get(index)
            .cloned()
            .ok_or_else(|| invalid(format!("No binding at index {index}")))?;
        let request = LearnRequest {
            target: current.target.clone(),
            learn_type: learn_type_for(current.mode),
            min: current.min,
            max: current.max,
            step: current.step,
            initial: current.initial,
            any_channel: current.channel == 0,
            invert: current.invert,
        };
        self.start_learn(request)
    }

    /// Learns a new binding for `target`: the next control that moves binds to it, with the
    /// target's default range and type.
    pub fn learn_new(&mut self, target: &str) -> Result<(), MidiError> {
        check_target(target)?;
        self.start_learn(LearnRequest::for_target(target))
    }

    fn start_learn(&mut self, request: LearnRequest) -> Result<(), MidiError> {
        self.pending_learn = Some(self.mapper.snapshot());
        self.mapper.arm_learn(request);
        Ok(())
    }

    pub fn cancel_learn(&mut self) {
        self.mapper.cancel_learn();
        self.pending_learn = None;
    }

    pub fn is_learning(&self) -> bool {
        self.mapper.is_learning()
    }

    /// Checks whether a pending learn has bound a control. Returns the new binding when one was
    /// captured (and records it for undo); call it from the UI tick.
    pub fn poll_learn(&mut self) -> Option<Mapping> {
        if self.mapper.is_learning() {
            return None;
        }
        let before = self.pending_learn.take()?;
        let after = self.mapper.snapshot();
        if after == before {
            return None;
        }
        let learned = after
            .mappings
            .iter()
            .find(|m| !before.mappings.contains(m))
            .cloned();
        self.record(before);
        learned
    }

    /// Commits a learn that finished on the MIDI thread so its undo step is recorded before the
    /// next edit.
    fn settle_learn(&mut self) {
        let _ = self.poll_learn();
    }

    /// Restores the map from before the last change. False when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.settle_learn();
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(self.mapper.snapshot());
        self.mapper.replace_map(previous);
        true
    }

    /// Re-applies the last undone change. False when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.settle_learn();
        let Some(next) = self.redo.pop() else {
            return false;
        };
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(self.mapper.snapshot());
        self.mapper.replace_map(next);
        true
    }

    /// Sets the map name and device hint (undoable).
    pub fn set_metadata(&mut self, name: &str, device: &str) -> Result<(), MidiError> {
        self.change(|m| {
            let mut map = m.snapshot();
            map.name = name.trim().to_owned();
            map.device = device.trim().to_owned();
            m.replace_map(map);
            Ok(())
        })
    }

    /// Replaces the live map with the empty default (no bindings, no feedback rules, no name).
    /// Undoable; the file is not touched until [`MapEditor::save`].
    pub fn reset_to_default(&mut self) -> Result<(), MidiError> {
        self.change(|m| {
            m.replace_map(MidiMap::default());
            Ok(())
        })
    }

    /// Replaces the live map with the one in `file`. A newer schema version, a bad binding or an
    /// oversized file changes nothing. Undoable; the change is unsaved until [`MapEditor::save`].
    pub fn import_file(&mut self, file: &Path) -> Result<(), MidiError> {
        let size = fs::metadata(file)
            .map_err(|_| MidiError::NotFound(file.to_path_buf()))?
            .len();
        if size > MAX_MAP_BYTES {
            return Err(invalid("Map file is larger than 1 MB; refusing to read it"));
        }
        let text = fs::read_to_string(file)?;
        let parsed = MidiMap::parse(&text)?;
        self.change(|m| {
            m.replace_map(parsed);
            Ok(())
        })
    }

    /// Writes the live map to `file` (atomically) without changing the save path or the dirty flag.
    pub fn export_file(&self, file: &Path) -> Result<(), MidiError> {
        write_atomic(file, &self.mapper.to_json(), false)
    }

    /// Writes the live map to the save path (atomically), keeping the previous file as `.bak`.
    /// Clears the dirty flag. Fails with a clear error when no map file is set.
    pub fn save(&mut self) -> Result<PathBuf, MidiError> {
        self.settle_learn();
        let path = self
            .path
            .clone()
            .ok_or_else(|| invalid("No map file is set; choose one in the MIDI settings first"))?;
        let json = self.mapper.to_json();
        write_atomic(&path, &json, true)?;
        self.saved_json = json;
        Ok(path)
    }
}

/// Writes `text` to a temporary sibling, syncs it, and renames it over `path`. With `keep_bak`,
/// the previous file is copied to `<path>.bak` first, so the old map survives a bad save.
pub fn write_atomic(path: &Path, text: &str, keep_bak: bool) -> Result<(), MidiError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    {
        let mut file = File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    if keep_bak && path.is_file() {
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        fs::copy(path, PathBuf::from(bak))?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
