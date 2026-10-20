//! View-model of the Mapping page: the binding table (filtered), the selection and edit form, the
//! conflicts list, the MIDI monitor lines and the Save and dirty state. Toolkit-free; the gpui
//! page renders these rows and forms. All edits go through [`om_midi::MapEditor`].

use om_core::ParamId;
use om_midi::{
    control_label, BindingDraft, Conflict, Keep, MapEditor, MapMode, Mapping, MidiEvent, MidiKind,
    MidiMapper, MonitorEntry,
};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

/// One row of the binding table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingRow {
    /// Index into the editor's bindings.
    pub index: usize,
    /// "Ch1 Note 36 -> Deck 1 Play [Button]".
    pub text: String,
    /// The target group, lower case: "deck", "mixer", ...
    pub group: String,
    /// Channel filter key: 0 = any.
    pub channel: i32,
    /// This row is part of a conflict.
    pub conflicted: bool,
}

/// What the table can be filtered by. Empty text and `None` match everything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MappingFilter {
    pub text: String,
    pub group: Option<String>,
    /// Bindings have no device of their own; this filters by channel (0 = any channel).
    pub channel: Option<i32>,
}

/// Button and mode label for a mapping mode.
pub fn mode_label(mode: MapMode) -> &'static str {
    match mode {
        MapMode::Absolute => "Absolute",
        MapMode::Relative => "Relative",
        MapMode::RelativeTwosComplement => "Relative 2C",
        MapMode::Button => "Button",
        MapMode::Toggle => "Toggle",
        MapMode::Absolute14 => "14-bit",
    }
}

/// A readable target: "deck.0.play" -> "Deck 1 Play", "mixer.ch0.fader" -> "Mixer Ch 1 Fader".
/// Numbers in the id are the router's zero-based indices and are shown one-based. An unknown id
/// is shown as it is written.
pub fn target_label(id: &str) -> String {
    if ParamId::from_str(id).is_err() {
        return id.to_owned();
    }
    id.split('.')
        .map(|segment| {
            if let Ok(n) = segment.parse::<u32>() {
                (n + 1).to_string()
            } else if let Some(rest) = segment.strip_prefix("ch") {
                match rest.parse::<u32>() {
                    Ok(n) => format!("Ch {}", n + 1),
                    Err(_) => capitalize(segment),
                }
            } else {
                capitalize(segment)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// "Ch1 Note 36 -> Deck 1 Play [Button]". Channel 0 reads "Any".
pub fn binding_text(m: &Mapping) -> String {
    let channel = if m.channel == 0 {
        "Any".to_owned()
    } else {
        format!("Ch{}", m.channel)
    };
    let control = match m.kind {
        MidiKind::Cc => format!("CC {}", m.number),
        MidiKind::Note => format!("Note {}", m.number),
        MidiKind::PitchBend => "Pitch bend".to_owned(),
    };
    format!(
        "{channel} {control} -> {} [{}]",
        target_label(&m.target),
        mode_label(m.mode)
    )
}

/// The group of a target id: its first segment, capitalised ("deck", "mixer", ...).
pub fn target_group(id: &str) -> String {
    id.split('.').next().unwrap_or_default().to_owned()
}

/// One monitor line for a received event: "CC 7 ch1 = 64", "Note on 36 ch1 vel 100".
pub fn event_text(e: &MidiEvent) -> String {
    match e.kind {
        MidiKind::Cc => format!("CC {} ch{} = {}", e.number, e.channel, e.value),
        MidiKind::Note if e.note_on => {
            format!("Note on {} ch{} vel {}", e.number, e.channel, e.value)
        }
        MidiKind::Note => format!("Note off {} ch{}", e.number, e.channel),
        MidiKind::PitchBend => format!("Pitch bend ch{} = {}", e.channel, e.value),
    }
}

/// The page model. Holds the editor, the filter, the selection and the edit form.
pub struct MappingEditorPage {
    editor: MapEditor,
    filter: MappingFilter,
    selected: Option<usize>,
    form: Option<BindingDraft>,
    status: String,
}

impl MappingEditorPage {
    pub fn new(mapper: Arc<MidiMapper>, path: Option<std::path::PathBuf>) -> Self {
        Self {
            editor: MapEditor::new(mapper, path),
            filter: MappingFilter::default(),
            selected: None,
            form: None,
            status: String::new(),
        }
    }

    pub fn editor(&self) -> &MapEditor {
        &self.editor
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn set_filter(&mut self, filter: MappingFilter) {
        self.filter = filter;
    }

    pub fn filter(&self) -> &MappingFilter {
        &self.filter
    }

    /// Target groups present in the map, for the group filter.
    pub fn groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = self
            .editor
            .bindings()
            .iter()
            .map(|m| target_group(&m.target))
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }

    /// The visible rows, in map order.
    pub fn rows(&self) -> Vec<MappingRow> {
        let conflicts = self.editor.conflicts();
        let needle = self.filter.text.trim().to_lowercase();
        self.editor
            .bindings()
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                let text = binding_text(m);
                (needle.is_empty() || text.to_lowercase().contains(&needle))
                    && self
                        .filter
                        .group
                        .as_ref()
                        .is_none_or(|g| *g == target_group(&m.target))
                    && self.filter.channel.is_none_or(|c| c == m.channel)
            })
            .map(|(index, m)| MappingRow {
                index,
                text: binding_text(m),
                group: target_group(&m.target),
                channel: m.channel,
                conflicted: conflicts
                    .iter()
                    .any(|c| c.first == index || c.second == index),
            })
            .collect()
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|&i| i < self.editor.bindings().len());
        self.form = self.selected.and_then(|i| {
            self.editor
                .bindings()
                .get(i)
                .map(BindingDraft::from_mapping)
        });
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The edit form for the selected binding (or the new binding being typed), if any.
    pub fn form(&self) -> Option<&BindingDraft> {
        self.form.as_ref()
    }

    pub fn form_mut(&mut self) -> Option<&mut BindingDraft> {
        self.form.as_mut()
    }

    /// An empty form for a new binding.
    pub fn new_form(&mut self) {
        self.selected = None;
        self.form = Some(BindingDraft {
            kind: MidiKind::Cc,
            channel: "0".into(),
            number: "0".into(),
            target: String::new(),
            mode: MapMode::Absolute,
            min: "0".into(),
            max: "1".into(),
            step: String::new(),
            initial: String::new(),
            invert: false,
            lsb: String::new(),
        });
    }

    /// Applies the form: to the selected binding, or as a new one when none is selected.
    pub fn apply_form(&mut self) -> Result<(), String> {
        let Some(draft) = self.form.clone() else {
            return Err("No binding is open for editing".into());
        };
        let result = match self.selected {
            Some(i) => self.editor.apply_draft(i, &draft),
            None => self.editor.add_draft(&draft).map(|i| {
                self.selected = Some(i);
            }),
        };
        self.report(result.map(|()| "Binding updated"))
    }

    pub fn delete_selected(&mut self) -> Result<(), String> {
        let index = self.selected.ok_or("Select a binding first")?;
        let result = self.editor.delete_binding(index);
        self.select(None);
        self.report(result.map(|()| "Binding deleted"))
    }

    pub fn duplicate_selected(&mut self) -> Result<(), String> {
        let index = self.selected.ok_or("Select a binding first")?;
        let result = self.editor.duplicate_binding(index);
        match result {
            Ok(copy) => {
                self.select(Some(copy));
                self.status = "Binding duplicated".into();
                Ok(())
            }
            Err(e) => self.report(Err::<&str, _>(e)),
        }
    }

    /// Re-learn the selected binding's control. Learning waits for a control to move.
    pub fn learn_selected(&mut self) -> Result<(), String> {
        let index = self.selected.ok_or("Select a binding first")?;
        let result = self.editor.learn_into(index);
        self.report(result.map(|()| "Move the control to re-learn it"))
    }

    /// Learn a new binding for `target` (a parameter id).
    pub fn learn_new(&mut self, target: &str) -> Result<(), String> {
        let result = self.editor.learn_new(target);
        self.report(result.map(|()| "Move the control to bind it"))
    }

    pub fn cancel_learn(&mut self) {
        self.editor.cancel_learn();
        self.status = "Learn cancelled".into();
    }

    pub fn is_learning(&self) -> bool {
        self.editor.is_learning()
    }

    /// Call from the UI tick. When a learn captured a control, selects its new binding.
    pub fn tick(&mut self) -> Option<Mapping> {
        let learned = self.editor.poll_learn()?;
        let index = self.editor.bindings().iter().position(|m| *m == learned);
        self.select(index);
        self.status = format!(
            "Learned {}",
            control_label(learned.kind, learned.channel, learned.number)
        );
        Some(learned)
    }

    pub fn undo(&mut self) -> Result<(), String> {
        let done = self.editor.undo();
        self.select(None);
        self.report(if done {
            Ok("Undone")
        } else {
            Err(om_midi::MidiError::Invalid("Nothing to undo".into()))
        })
    }

    pub fn redo(&mut self) -> Result<(), String> {
        let done = self.editor.redo();
        self.select(None);
        self.report(if done {
            Ok("Redone")
        } else {
            Err(om_midi::MidiError::Invalid("Nothing to redo".into()))
        })
    }

    pub fn save(&mut self) -> Result<(), String> {
        let result = self
            .editor
            .save()
            .map(|path| format!("Saved {}", path.display()));
        self.report_text(result)
    }

    pub fn import(&mut self, file: &Path) -> Result<(), String> {
        let result = self.editor.import_file(file);
        self.select(None);
        self.report(result.map(|()| "Imported (not saved yet)"))
    }

    pub fn export(&mut self, file: &Path) -> Result<(), String> {
        let result = self.editor.export_file(file);
        self.report(result.map(|()| "Exported"))
    }

    pub fn reset_to_default(&mut self) -> Result<(), String> {
        let result = self.editor.reset_to_default();
        self.select(None);
        self.report(result.map(|()| "Reset to the empty map (not saved yet)"))
    }

    /// Conflicts in the live map, for the conflicts list.
    pub fn conflicts(&self) -> Vec<Conflict> {
        self.editor.conflicts()
    }

    pub fn resolve_conflict(&mut self, conflict: &Conflict, keep: Keep) -> Result<(), String> {
        let result = self.editor.resolve_conflict(conflict, keep);
        self.select(None);
        self.report(result.map(|()| "Conflict resolved"))
    }

    /// True when leaving the page would lose edits: the guard asks before it closes.
    pub fn needs_save_prompt(&self) -> bool {
        self.editor.is_dirty()
    }

    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    /// The last `count` monitor lines, newest first, each with its age: "CC 7 ch1 = 64 (0.4 s)".
    pub fn monitor_lines(&self, count: usize) -> Vec<String> {
        monitor_lines(&self.editor.mapper().recent_events(), Instant::now(), count)
    }

    /// The most recent event, for the one-line monitor.
    pub fn last_event_line(&self) -> String {
        match self.editor.mapper().recent_events().last() {
            Some(entry) => format!("Last: {}", event_text(&entry.event)),
            None => "Last: no MIDI received yet".into(),
        }
    }

    fn report(&mut self, result: Result<&str, om_midi::MidiError>) -> Result<(), String> {
        match result {
            Ok(message) => {
                self.status = message.to_owned();
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                self.status = message.clone();
                Err(message)
            }
        }
    }

    fn report_text(&mut self, result: Result<String, om_midi::MidiError>) -> Result<(), String> {
        match result {
            Ok(message) => {
                self.status = message;
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                self.status = message.clone();
                Err(message)
            }
        }
    }
}

/// Formats the newest `count` monitor entries, newest first, with each one's age.
pub fn monitor_lines(entries: &[MonitorEntry], now: Instant, count: usize) -> Vec<String> {
    entries
        .iter()
        .rev()
        .take(count)
        .map(|entry| {
            let age = now.saturating_duration_since(entry.at).as_secs_f64();
            format!("{} ({age:.1} s ago)", event_text(&entry.event))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn page() -> (MappingEditorPage, Arc<MidiMapper>) {
        let mapper = Arc::new(MidiMapper::new());
        let page = MappingEditorPage::new(Arc::clone(&mapper), None);
        (page, mapper)
    }

    fn cc(channel: i32, number: i32, target: &str) -> Mapping {
        Mapping {
            kind: MidiKind::Cc,
            channel,
            number,
            target: target.into(),
            ..Mapping::default()
        }
    }

    #[test]
    fn row_text_matches_the_spec_example() {
        let m = Mapping {
            kind: MidiKind::Note,
            channel: 1,
            number: 36,
            target: "deck.0.play".into(),
            mode: MapMode::Toggle,
            ..Mapping::default()
        };
        assert_eq!(binding_text(&m), "Ch1 Note 36 -> Deck 1 Play [Toggle]");
        let any = Mapping {
            kind: MidiKind::Note,
            channel: 0,
            number: 36,
            target: "deck.0.play".into(),
            mode: MapMode::Button,
            ..Mapping::default()
        };
        assert_eq!(binding_text(&any), "Any Note 36 -> Deck 1 Play [Button]");
    }

    #[test]
    fn target_labels_are_one_based_and_unknown_ids_pass_through() {
        assert_eq!(target_label("mixer.ch0.fader"), "Mixer Ch 1 Fader");
        assert_eq!(target_label("deck.1.rate"), "Deck 2 Rate");
        assert_eq!(target_label("nonsense..id"), "nonsense..id");
    }

    #[test]
    fn modes_and_kinds_are_labelled() {
        let mut wide = cc(0, 1, "mixer.ch0.fader");
        wide.mode = MapMode::Absolute14;
        wide.lsb_number = 33;
        assert!(binding_text(&wide).ends_with("[14-bit]"));
        let mut rel = cc(0, 21, "mixer.ch0.eq.low");
        rel.mode = MapMode::Relative;
        assert!(binding_text(&rel).ends_with("[Relative]"));
        let mut two = cc(0, 22, "mixer.ch0.eq.low");
        two.mode = MapMode::RelativeTwosComplement;
        assert!(binding_text(&two).ends_with("[Relative 2C]"));
    }

    #[test]
    fn filter_by_text_group_and_channel() {
        let (mut page, mapper) = page();
        mapper.add_binding(cc(1, 7, "mixer.ch0.fader")).unwrap();
        mapper.add_binding(cc(2, 8, "deck.0.rate")).unwrap();
        mapper.add_binding(cc(0, 9, "deck.1.rate")).unwrap();
        assert_eq!(page.rows().len(), 3);

        page.set_filter(MappingFilter {
            group: Some("deck".into()),
            ..MappingFilter::default()
        });
        assert_eq!(
            page.rows().iter().map(|r| r.index).collect::<Vec<_>>(),
            vec![1, 2]
        );

        page.set_filter(MappingFilter {
            text: "CC 7".into(),
            ..MappingFilter::default()
        });
        assert_eq!(page.rows().len(), 1);

        page.set_filter(MappingFilter {
            channel: Some(0),
            ..MappingFilter::default()
        });
        assert_eq!(page.rows()[0].index, 2);
        assert_eq!(page.groups(), vec!["deck".to_string(), "mixer".to_string()]);
    }

    #[test]
    fn rows_flag_conflicts() {
        let (page, mapper) = page();
        mapper.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
        mapper.add_binding(cc(3, 7, "mixer.ch1.fader")).unwrap();
        mapper.add_binding(cc(0, 9, "mixer.ch2.fader")).unwrap();
        let flags: Vec<bool> = page.rows().iter().map(|r| r.conflicted).collect();
        assert_eq!(flags, vec![true, true, false]);
    }

    #[test]
    fn form_edits_apply_and_report_errors() {
        let (mut page, mapper) = page();
        mapper.add_binding(cc(1, 7, "mixer.ch0.fader")).unwrap();
        page.select(Some(0));
        assert_eq!(page.form().unwrap().number, "7");

        page.form_mut().unwrap().number = "11".into();
        page.apply_form().unwrap();
        assert_eq!(mapper.bindings()[0].number, 11);

        page.form_mut().unwrap().target = "deck.0.nonsense".into();
        let err = page.apply_form().unwrap_err();
        assert!(err.contains("deck.0.nonsense"), "{err}");
        assert_eq!(
            mapper.bindings()[0].target,
            "mixer.ch0.fader",
            "a bad form changes nothing"
        );
        assert_eq!(page.status(), err);
    }

    #[test]
    fn add_delete_duplicate_undo_redo_through_the_page() {
        let (mut page, mapper) = page();
        page.new_form();
        page.form_mut().unwrap().target = "deck.0.play".into();
        page.form_mut().unwrap().kind = MidiKind::Note;
        page.form_mut().unwrap().number = "36".into();
        page.form_mut().unwrap().mode = MapMode::Toggle;
        page.apply_form().unwrap();
        assert_eq!(mapper.bindings().len(), 1);

        page.duplicate_selected().unwrap();
        assert_eq!(mapper.bindings().len(), 2);
        assert_eq!(page.selected(), Some(1));

        page.delete_selected().unwrap();
        assert_eq!(mapper.bindings().len(), 1);
        page.undo().unwrap();
        assert_eq!(mapper.bindings().len(), 2);
        page.redo().unwrap();
        assert_eq!(mapper.bindings().len(), 1);
        assert!(page.redo().is_err());
    }

    #[test]
    fn learn_into_selects_the_new_binding_on_tick() {
        let (mut page, mapper) = page();
        mapper.add_binding(cc(1, 7, "mixer.ch0.fader")).unwrap();
        page.select(Some(0));
        page.learn_selected().unwrap();
        assert!(page.is_learning());
        mapper.handle_message(&om_midi::control_change(2, 21, 64));
        let learned = page.tick().expect("captured");
        assert_eq!(learned.number, 21);
        assert_eq!(page.selected(), Some(0));
        assert!(page.status().starts_with("Learned"));
    }

    #[test]
    fn dirty_state_drives_the_save_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let mapper = Arc::new(MidiMapper::new());
        let mut page = MappingEditorPage::new(Arc::clone(&mapper), Some(dir.path().join("m.json")));
        assert!(!page.needs_save_prompt());
        mapper.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
        assert!(page.needs_save_prompt());
        page.save().unwrap();
        assert!(!page.is_dirty());
        assert!(page.status().starts_with("Saved"));
    }

    #[test]
    fn monitor_is_newest_first_with_ages() {
        let (page, mapper) = page();
        assert_eq!(page.last_event_line(), "Last: no MIDI received yet");
        mapper.handle_message(&om_midi::control_change(1, 7, 64));
        mapper.handle_message(&om_midi::note_on(1, 36, 100));
        assert_eq!(page.last_event_line(), "Last: Note on 36 ch1 vel 100");
        let lines = page.monitor_lines(5);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("Note on 36"));
        assert!(lines[1].starts_with("CC 7 ch1 = 64"));
        let later = Instant::now() + Duration::from_millis(1500);
        let aged = monitor_lines(&mapper.recent_events(), later, 1);
        assert!(aged[0].ends_with("s ago)"), "{aged:?}");
    }

    #[test]
    fn import_refusal_leaves_the_page_status_as_the_error() {
        let (mut page, _mapper) = page();
        let dir = tempfile::tempdir().unwrap();
        let newer = dir.path().join("newer.json");
        std::fs::write(&newer, r#"{"version":3,"mappings":[]}"#).unwrap();
        assert!(page.import(&newer).is_err());
        assert!(page.status().contains("version 3"));
    }
}
