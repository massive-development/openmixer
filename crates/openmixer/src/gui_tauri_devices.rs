//! The Tauri window's MIDI, mapping and plugins pages: the same `om-ui` page logic the GPUI
//! settings tabs used (`MidiSettingsPage`, `MappingEditorPage`, the plugin host over the
//! composition), turned into the page data the front end draws.

use crate::gui_tauri_settings::PrefsStore;
use om_midi::{BindingDraft, Conflict, Keep, MapMode, MidiKind};
use om_ui::labels::param_label;
use om_ui::mapping_editor::{mode_label, MappingEditorPage, MappingFilter as UiFilter};
use om_ui::preferences::{ClapScan, Preferences};
use om_ui::settings::{learnable_targets, MidiHost, MidiInputs, MidiSettingsPage, PluginHost};
use om_ui::theme::Source;
use om_ui_tauri::{
    Choice, ClapSection, DevicesHost, MappingChange, MappingConflict, MappingFilter, MappingForm,
    MappingPage, MappingRow, MidiChange, MidiPage, Notice, PluginParam, PluginRow, PluginsChange,
    PluginsPage, View,
};
use openmixer::composition::AppComposition;
use openmixer::midi::MidiService;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// Lines the MIDI monitor keeps, newest first.
const MONITOR_LINES: usize = 50;

const KINDS: [(MidiKind, &str, &str); 3] = [
    (MidiKind::Cc, "cc", "CC"),
    (MidiKind::Note, "note", "Note"),
    (MidiKind::PitchBend, "pitchBend", "Pitch bend"),
];

const MODES: [MapMode; 6] = [
    MapMode::Absolute,
    MapMode::Relative,
    MapMode::RelativeTwosComplement,
    MapMode::Button,
    MapMode::Toggle,
    MapMode::Absolute14,
];

pub struct Devices {
    store: Rc<PrefsStore>,
    /// The learn that is waiting: its target and the map's generation when it was armed.
    learning: RefCell<Option<(String, u64)>>,
    mapping: RefCell<Option<MappingEditorPage>>,
}

impl Devices {
    pub fn new(store: Rc<PrefsStore>) -> Self {
        Self {
            store,
            learning: RefCell::new(None),
            mapping: RefCell::new(None),
        }
    }

    fn composition(&self) -> &Rc<RefCell<AppComposition>> {
        self.store.composition()
    }

    fn midi_service(&self) -> Option<Arc<MidiService>> {
        self.composition().borrow().services.midi.clone()
    }

    fn prefs(&self) -> Preferences {
        self.composition().borrow().prefs.clone()
    }

    fn edit_prefs(&self, edit: impl FnOnce(&mut Preferences) -> bool) {
        self.store.edit_prefs(edit);
    }

    // ---- MIDI ----

    fn midi_page(&self) -> MidiPage {
        let Some(midi) = self.midi_service() else {
            return MidiPage::default();
        };
        let composition = self.composition().borrow();
        let prefs = &composition.prefs;
        let decks = composition.engine.num_decks();
        MidiPage {
            available: true,
            inputs: MidiInputs::input_names(&*midi),
            enable_all: prefs.midi.enable_all,
            enabled_inputs: prefs.midi.enabled_inputs.clone(),
            map_file: prefs.effective_map_file().display().to_string(),
            targets: learnable_targets(decks)
                .into_iter()
                .map(|id| Choice::new(id.clone(), param_label(&id)))
                .collect(),
            learning: self.learning.borrow().as_ref().map(|(t, _)| t.clone()),
        }
    }

    /// When the armed learn has ended, says what happened (a saved map, or nothing received).
    fn poll_learn(&self, midi: &MidiService) -> Option<Notice> {
        let (target, generation) = self.learning.borrow().clone()?;
        if midi.is_learning() {
            return None;
        }
        *self.learning.borrow_mut() = None;
        let label = param_label(&target);
        if midi.map_generation() == generation {
            return Some(Notice::error(format!(
                "Learn for {label} ended: no control was received"
            )));
        }
        Some(self.save_learned(midi, &label))
    }

    /// Saves the live map after a learn and makes its file the map in use.
    fn save_learned(&self, midi: &MidiService, label: &str) -> Notice {
        let mut saved = None;
        self.edit_prefs(|prefs| {
            let result = MidiSettingsPage::new(prefs, midi).save_learned_map(midi);
            let changed = matches!(result, Ok((_, true)));
            saved = Some(result);
            changed
        });
        match saved {
            Some(Ok((file, _))) => Notice::info(format!(
                "Learned a control for {label} and saved to {}",
                file.display()
            )),
            Some(Err(error)) => Notice::error(format!(
                "Learned a control for {label}, but the map was not saved: {error}"
            )),
            None => Notice::error("MIDI is not available"),
        }
    }

    fn set_map_file(&self, file: Option<PathBuf>) {
        // Read before the edit: the edit holds the composition.
        if let Some(midi) = self.midi_service() {
            self.edit_prefs(|prefs| MidiSettingsPage::new(prefs, &*midi).set_map_file(file));
        }
        // The editor keeps the path it was made with.
        *self.mapping.borrow_mut() = None;
    }

    fn apply_midi(&self, midi: &Arc<MidiService>, change: MidiChange) -> Option<Notice> {
        match change {
            MidiChange::EnableAll(all) => {
                self.edit_prefs(|p| MidiSettingsPage::new(p, &**midi).set_enable_all(all));
                None
            }
            MidiChange::ToggleInput(name) => {
                self.edit_prefs(|p| {
                    let on = !p.midi.enabled_inputs.contains(&name);
                    MidiSettingsPage::new(p, &**midi).set_input_enabled(&name, on)
                });
                None
            }
            MidiChange::LoadMap(path) => Some(self.load_map(midi, PathBuf::from(path))),
            MidiChange::LoadCurrentMap => Some(self.load_current(midi)),
            MidiChange::UseDefaultMap => {
                self.set_map_file(None);
                Some(Notice::info(
                    "The default map file is used from the next load",
                ))
            }
            MidiChange::Learn(target) => Some(self.arm(midi, &target)),
            MidiChange::CancelLearn => {
                midi.cancel_learn();
                *self.learning.borrow_mut() = None;
                Some(Notice::info("Learn cancelled"))
            }
        }
    }

    /// Loads `file` as the controller map; the preference is stored only when the file loads.
    fn load_map(&self, midi: &MidiService, file: PathBuf) -> Notice {
        match midi.load_map(&file) {
            Ok(()) => {
                let shown = file.display().to_string();
                self.set_map_file(Some(file));
                Notice::info(format!("Loaded the map {shown}"))
            }
            Err(error) => Notice::error(format!("Map not loaded: {error}")),
        }
    }

    fn load_current(&self, midi: &MidiService) -> Notice {
        let file = self.prefs().effective_map_file();
        match midi.load_map(&file) {
            Ok(()) => {
                *self.mapping.borrow_mut() = None;
                Notice::info(format!("Loaded the map {}", file.display()))
            }
            Err(error) => Notice::error(format!("Map not loaded: {error}")),
        }
    }

    fn arm(&self, midi: &MidiService, target: &str) -> Notice {
        let generation = midi.map_generation();
        midi.arm_learn(target);
        *self.learning.borrow_mut() = Some((target.to_owned(), generation));
        Notice::info(format!(
            "Learning {}: move a control on the controller. Cancel stops it.",
            param_label(target)
        ))
    }

    // ---- Mapping ----

    /// The editor, made on first use over the live mapper; `None` without MIDI.
    fn with_mapping<R>(&self, f: impl FnOnce(&mut MappingEditorPage) -> R) -> Option<R> {
        let mut slot = self.mapping.borrow_mut();
        if slot.is_none() {
            *slot = self.composition().borrow().mapping_editor();
        }
        slot.as_mut().map(f)
    }

    fn mapping_page(&self) -> MappingPage {
        let map_file = self.prefs().effective_map_file().display().to_string();
        let learnable = learnable_targets(self.composition().borrow().engine.num_decks());
        self.with_mapping(|page| {
            let _ = page.tick();
            describe_mapping(page, map_file, &learnable)
        })
        .unwrap_or_default()
    }

    fn apply_mapping(&self, change: MappingChange) -> Option<Notice> {
        self.with_mapping(|page| {
            let result = run_mapping(page, change)?;
            Some(match result {
                Ok(()) if page.status().is_empty() => return None,
                Ok(()) => Notice::info(page.status()),
                Err(error) => Notice::error(error),
            })
        })
        .flatten()
    }

    // ---- Plugins ----

    fn plugins_page(&self) -> PluginsPage {
        let composition = self.composition().borrow();
        let prefs = &composition.prefs;
        let loaded = composition
            .plugins
            .plugin_ids()
            .into_iter()
            .map(|id| plugin_row(&composition, id))
            .collect();
        PluginsPage {
            available: true,
            folder: prefs.effective_plugin_folder().display().to_string(),
            loaded,
            builtins: PluginHost::builtins(&*composition)
                .into_iter()
                .map(|(id, name)| Choice::new(id, name))
                .collect(),
            clap: clap_section(&composition),
            theme_count: composition
                .themes
                .themes()
                .iter()
                .filter(|e| e.source == Source::Plugin)
                .count(),
        }
    }

    fn apply_plugins(&self, change: PluginsChange) -> Option<Notice> {
        let outcome = match change {
            PluginsChange::Refresh => return None,
            PluginsChange::Reload => {
                self.run_plugin(|c| c.reload(), "Reloaded the plugin folder".into())
            }
            PluginsChange::LoadBuiltin(id) => {
                self.run_plugin(|c| c.load_builtin(&id), format!("Loaded {id}"))
            }
            PluginsChange::Unload(id) => {
                self.run_plugin(|c| c.unload(&id), format!("Unloaded {id}"))
            }
            PluginsChange::SetParam { id, param, value } => {
                let result = self
                    .composition()
                    .borrow_mut()
                    .set_plugin_parameter(&id, &param, value);
                result.map(|()| None).map_err(|e| e.to_string())
            }
            PluginsChange::ClapScan(allow) => Ok(self.set_clap_scan(allow)),
            PluginsChange::ClapAcceptWarning => Ok(self.accept_clap_warning()),
        };
        match outcome {
            Ok(notice) => notice,
            Err(error) => Some(Notice::error(error)),
        }
    }

    fn run_plugin(
        &self,
        f: impl FnOnce(&mut AppComposition) -> om_ui::settings::SettingsResult<()>,
        done: String,
    ) -> Result<Option<Notice>, String> {
        let result = f(&mut self.composition().borrow_mut());
        result.map_err(|e| e.to_string())?;
        Ok(Some(Notice::info(done)))
    }

    fn set_clap_scan(&self, allow: bool) -> Option<Notice> {
        self.edit_prefs(|p| {
            p.plugins.clap_scan = if allow {
                ClapScan::Allowed
            } else {
                ClapScan::Declined
            };
            true
        });
        Some(Notice::info(if allow {
            "Scanning the standard CLAP folders"
        } else {
            "Only the plugin folder is scanned for CLAP plugins"
        }))
    }

    fn accept_clap_warning(&self) -> Option<Notice> {
        self.edit_prefs(|p| {
            p.plugins.clap_warning_accepted = true;
            true
        });
        Some(Notice::info("Loading CLAP plugins"))
    }
}

impl DevicesHost for Devices {
    fn midi(&self) -> View<MidiPage> {
        let notice = self.midi_service().and_then(|midi| self.poll_learn(&midi));
        View {
            page: self.midi_page(),
            notice,
        }
    }

    fn change_midi(&self, change: MidiChange) -> View<MidiPage> {
        let notice = match self.midi_service() {
            Some(midi) => self.apply_midi(&midi, change),
            None => Some(Notice::error("MIDI is not available")),
        };
        View {
            page: self.midi_page(),
            notice,
        }
    }

    fn mapping(&self) -> View<MappingPage> {
        View {
            page: self.mapping_page(),
            notice: None,
        }
    }

    fn change_mapping(&self, change: MappingChange) -> View<MappingPage> {
        let notice = self.apply_mapping(change);
        View {
            page: self.mapping_page(),
            notice,
        }
    }

    fn plugins(&self) -> View<PluginsPage> {
        View {
            page: self.plugins_page(),
            notice: None,
        }
    }

    fn change_plugins(&self, change: PluginsChange) -> View<PluginsPage> {
        let notice = self.apply_plugins(change);
        View {
            page: self.plugins_page(),
            notice,
        }
    }

    fn plugin_window(&self, id: &str) -> Option<Arc<dyn om_ui::settings::PluginWindow>> {
        PluginHost::plugin_window(&*self.composition().borrow(), id)
    }
}

// ---- Page data ----

fn describe_mapping(
    page: &MappingEditorPage,
    map_file: String,
    learnable: &[String],
) -> MappingPage {
    let filter = page.filter();
    MappingPage {
        available: true,
        map_file,
        dirty: page.is_dirty(),
        learning: page.is_learning(),
        filter: MappingFilter {
            text: filter.text.clone(),
            group: filter.group.clone(),
            channel: filter.channel,
        },
        groups: page.groups(),
        rows: page
            .rows()
            .into_iter()
            .map(|r| MappingRow {
                index: r.index,
                text: r.text,
                group: r.group,
                conflicted: r.conflicted,
            })
            .collect(),
        selected: page.selected(),
        form: page.form().map(form_dto),
        conflicts: page
            .conflicts()
            .into_iter()
            .map(|c| MappingConflict {
                first: c.first,
                second: c.second,
                control: c.control,
            })
            .collect(),
        monitor: page.monitor_lines(MONITOR_LINES),
        kinds: KINDS
            .iter()
            .map(|(_, id, label)| Choice::new(*id, *label))
            .collect(),
        modes: MODES
            .iter()
            .map(|m| Choice::new(m.as_str(), mode_label(*m)))
            .collect(),
        targets: learnable
            .iter()
            .map(|id| Choice::new(id.clone(), param_label(id)))
            .collect(),
    }
}

fn kind_id(kind: MidiKind) -> &'static str {
    KINDS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map_or("cc", |(_, id, _)| id)
}

fn form_dto(draft: &BindingDraft) -> MappingForm {
    MappingForm {
        kind: kind_id(draft.kind).to_owned(),
        channel: draft.channel.clone(),
        number: draft.number.clone(),
        target: draft.target.clone(),
        mode: draft.mode.as_str().to_owned(),
        min: draft.min.clone(),
        max: draft.max.clone(),
        step: draft.step.clone(),
        initial: draft.initial.clone(),
        invert: draft.invert,
        lsb: draft.lsb.clone(),
        error: draft.to_mapping().err().map(|e| e.to_string()),
    }
}

/// The form's fields onto `draft`; an unknown kind or mode keeps what the draft had.
fn fill_draft(draft: &mut BindingDraft, form: MappingForm) {
    if let Some((kind, _, _)) = KINDS.iter().find(|(_, id, _)| *id == form.kind) {
        draft.kind = *kind;
    }
    if let Some(mode) = MODES.iter().find(|m| m.as_str() == form.mode) {
        draft.mode = *mode;
    }
    draft.channel = form.channel;
    draft.number = form.number;
    draft.target = form.target;
    draft.min = form.min;
    draft.max = form.max;
    draft.step = form.step;
    draft.initial = form.initial;
    draft.invert = form.invert;
    draft.lsb = form.lsb;
}

/// Runs one mapping edit on the editor; `None` for a change that only re-reads the page.
fn run_mapping(page: &mut MappingEditorPage, change: MappingChange) -> Option<Result<(), String>> {
    Some(match change {
        MappingChange::Refresh => return None,
        MappingChange::SetFilter(f) => {
            page.set_filter(UiFilter {
                text: f.text,
                group: f.group,
                channel: f.channel,
            });
            return None;
        }
        MappingChange::Select(index) => {
            page.select(index);
            return None;
        }
        MappingChange::New => {
            page.new_form();
            return None;
        }
        MappingChange::SetForm(form) => {
            if let Some(draft) = page.form_mut() {
                fill_draft(draft, *form);
            }
            return None;
        }
        MappingChange::Apply => page.apply_form(),
        MappingChange::Duplicate => page.duplicate_selected(),
        MappingChange::Delete => page.delete_selected(),
        MappingChange::LearnSelected => page.learn_selected(),
        MappingChange::LearnNew(target) => page.learn_new(&target),
        MappingChange::CancelLearn => {
            page.cancel_learn();
            Ok(())
        }
        MappingChange::Undo => page.undo(),
        MappingChange::Redo => page.redo(),
        MappingChange::Save => page.save(),
        MappingChange::Import(path) => page.import(&PathBuf::from(path)),
        MappingChange::Export(path) => page.export(&PathBuf::from(path)),
        MappingChange::Reset => page.reset_to_default(),
        MappingChange::Resolve {
            first,
            second,
            keep,
        } => {
            let conflict = Conflict {
                first,
                second,
                control: String::new(),
            };
            let keep = if keep == "second" {
                Keep::Second
            } else {
                Keep::First
            };
            page.resolve_conflict(&conflict, keep)
        }
    })
}

fn plugin_row(composition: &AppComposition, id: String) -> PluginRow {
    let params = PluginHost::plugin_parameters(composition, &id)
        .into_iter()
        .map(|p| PluginParam {
            text: p.display(),
            id: p.id,
            name: p.name,
            min: p.min,
            max: p.max,
            default: p.default,
            value: p.value,
        })
        .collect();
    PluginRow {
        latency: PluginHost::plugin_latency(composition, &id).map(|l| l.labelled()),
        has_window: has_window(composition, &id),
        params,
        id,
    }
}

#[cfg(feature = "clap")]
fn has_window(composition: &AppComposition, id: &str) -> bool {
    composition.plugins.clap_gui(id).is_some()
}

#[cfg(not(feature = "clap"))]
fn has_window(_composition: &AppComposition, _id: &str) -> bool {
    false
}

fn clap_section(composition: &AppComposition) -> Option<ClapSection> {
    let status = PluginHost::clap_status(composition);
    if !status.supported {
        return None;
    }
    let plugins = &composition.prefs.plugins;
    Some(ClapSection {
        scan: match plugins.clap_scan {
            ClapScan::Unasked => "unasked",
            ClapScan::Allowed => "allowed",
            ClapScan::Declined => "declined",
        }
        .to_owned(),
        standard_folders: status.standard_folders,
        files: status.files,
        waiting: status.waiting,
        warning_accepted: plugins.clap_warning_accepted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui_tauri::DevicesHost;
    use openmixer::startup::mock_output;
    use tempfile::TempDir;

    /// The pages over a composition with a mock controller, a mock output and a preferences file
    /// in `dir`, so nothing reaches a device or the user's files.
    fn devices(dir: &TempDir) -> Devices {
        let prefs_file = dir.path().join("prefs.json");
        let mut composition = AppComposition::with_defaults();
        let midi = om_midi::MockBackend::new();
        midi.add_input("Mock controller");
        composition.attach_midi(Box::new(midi));
        composition.prefs.midi.map_file =
            dir.path().join("map.json").to_string_lossy().into_owned();
        let store = PrefsStore::new(
            Rc::new(RefCell::new(composition)),
            mock_output(),
            prefs_file,
        );
        Devices::new(Rc::new(store))
    }

    fn form(target: &str) -> MappingForm {
        MappingForm {
            kind: "cc".into(),
            channel: "1".into(),
            number: "7".into(),
            target: target.into(),
            mode: "absolute".into(),
            min: "0".into(),
            max: "1".into(),
            ..MappingForm::default()
        }
    }

    #[test]
    fn the_midi_page_lists_the_controller_and_stores_the_input_choice() {
        let dir = TempDir::new().unwrap();
        let d = devices(&dir);
        let page = d.midi().page;
        assert!(page.available && page.enable_all);
        assert_eq!(page.inputs, vec!["Mock controller"]);
        assert!(page.targets.iter().any(|t| t.id == "mixer.crossfader"));
        d.change_midi(MidiChange::EnableAll(false));
        let page = d
            .change_midi(MidiChange::ToggleInput("Mock controller".into()))
            .page;
        assert!(!page.enable_all);
        assert_eq!(page.enabled_inputs, vec!["Mock controller"]);
        assert!(
            dir.path().join("prefs.json").exists(),
            "the choice is saved"
        );
    }

    #[test]
    fn learn_arms_a_target_and_cancel_clears_it() {
        let dir = TempDir::new().unwrap();
        let d = devices(&dir);
        let view = d.change_midi(MidiChange::Learn("mixer.crossfader".into()));
        assert_eq!(view.page.learning.as_deref(), Some("mixer.crossfader"));
        assert!(view.notice.is_some_and(|n| !n.error));
        let view = d.change_midi(MidiChange::CancelLearn);
        assert_eq!(view.page.learning, None);
    }

    #[test]
    fn a_map_that_does_not_load_changes_nothing_and_says_why() {
        let dir = TempDir::new().unwrap();
        let d = devices(&dir);
        let before = d.midi().page.map_file;
        let missing = dir.path().join("nope.json").to_string_lossy().into_owned();
        let view = d.change_midi(MidiChange::LoadMap(missing));
        assert!(view.notice.is_some_and(|n| n.error));
        assert_eq!(view.page.map_file, before);
    }

    #[test]
    fn a_binding_is_typed_validated_applied_and_saved() {
        let dir = TempDir::new().unwrap();
        let d = devices(&dir);
        d.change_mapping(MappingChange::New);
        let view = d.change_mapping(MappingChange::SetForm(Box::new(form(""))));
        assert!(
            view.page.form.is_some_and(|f| f.error.is_some()),
            "no target yet"
        );
        d.change_mapping(MappingChange::SetForm(Box::new(form("mixer.crossfader"))));
        let view = d.change_mapping(MappingChange::Apply);
        assert_eq!(view.page.rows.len(), 1);
        assert!(view.page.rows[0].text.contains("Crossfader"));
        assert!(view.page.dirty);
        let view = d.change_mapping(MappingChange::Save);
        assert!(!view.page.dirty);
        assert!(dir.path().join("map.json").exists());
        let view = d.change_mapping(MappingChange::Undo);
        assert!(view.page.rows.is_empty());
    }

    #[test]
    fn the_form_round_trips_through_a_draft() {
        let mut draft = BindingDraft::from_mapping(&om_midi::Mapping {
            kind: MidiKind::Note,
            channel: 2,
            number: 36,
            target: "deck.0.play".into(),
            mode: MapMode::Button,
            min: 0.0,
            max: 1.0,
            step: 0.0,
            initial: 0.0,
            invert: false,
            lsb_number: -1,
        });
        let mut dto = form_dto(&draft);
        assert_eq!((dto.kind.as_str(), dto.mode.as_str()), ("note", "button"));
        dto.kind = "pitchBend".into();
        dto.mode = "absolute14".into();
        dto.lsb = "39".into();
        fill_draft(&mut draft, dto);
        assert_eq!(
            (draft.kind, draft.mode),
            (MidiKind::PitchBend, MapMode::Absolute14)
        );
        assert_eq!(draft.lsb, "39");
        let mut bogus = form_dto(&draft);
        bogus.kind = "x".into();
        fill_draft(&mut draft, bogus);
        assert_eq!(
            draft.kind,
            MidiKind::PitchBend,
            "an unknown kind changes nothing"
        );
    }

    #[test]
    fn a_built_in_plugin_loads_and_unloads_and_a_bad_edit_says_why() {
        let dir = TempDir::new().unwrap();
        let d = devices(&dir);
        let page = d.plugins().page;
        let id = page.builtins.first().expect("a built-in plugin").id.clone();
        let page = d
            .change_plugins(PluginsChange::LoadBuiltin(id.clone()))
            .page;
        assert!(page.loaded.iter().any(|p| p.id == id), "loaded");
        assert!(
            page.builtins.iter().all(|b| b.id != id),
            "no longer offered"
        );
        let view = d.change_plugins(PluginsChange::Unload(id.clone()));
        assert!(view.page.loaded.is_empty() && view.page.builtins.iter().any(|b| b.id == id));
        let view = d.change_plugins(PluginsChange::Unload(id));
        assert!(
            view.notice.is_some_and(|n| n.error),
            "unloading what is not loaded says so"
        );
        let bad = PluginsChange::SetParam {
            id: "nope".into(),
            param: "x".into(),
            value: 1.0,
        };
        assert!(d.change_plugins(bad).notice.is_some_and(|n| n.error));
    }
}
