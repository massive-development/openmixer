//! The Mapping tab: the MIDI mapping editor. The logic is om-ui's `MappingEditorPage` (every edit
//! goes through om-midi's `MapEditor` onto the live mapper); this file draws the toolbar, filter,
//! binding table, edit form, conflicts and MIDI monitor, and forwards clicks to the methods on
//! [`SettingsState`] below. Save writes the map file; Import and Export take a typed path.

use super::chips::*;
use super::*;
use om_midi::{Conflict, Keep, MapMode, MidiKind};
use om_ui::mapping_editor::{mode_label, MappingFilter};

/// One field of the edit form, typed into the text field at the foot of the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapField {
    Channel,
    Number,
    Min,
    Max,
    Target,
}

impl MapField {
    pub const ALL: [MapField; 5] = [
        MapField::Channel,
        MapField::Number,
        MapField::Min,
        MapField::Max,
        MapField::Target,
    ];

    fn label(self) -> &'static str {
        match self {
            MapField::Channel => "Channel",
            MapField::Number => "Number",
            MapField::Min => "Min",
            MapField::Max => "Max",
            MapField::Target => "Target",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            MapField::Channel => "MIDI channel (1-16, 0 = any)",
            MapField::Number => "controller or note number (0-127)",
            MapField::Min => "minimum value",
            MapField::Max => "maximum value",
            MapField::Target => "pick a control or type its id under Advanced",
        }
    }
}

/// A confirm step the tab shows before an action that would lose work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingConfirm {
    /// Reset replaces every binding with an empty map (undoable, but not saved).
    Reset,
    /// The overlay is closing with unsaved edits.
    Leave,
}

/// Rows the binding table shows at once; the table scrolls.
const TABLE_HEIGHT: f32 = 150.0;
/// Lines the MIDI monitor keeps on screen, newest first.
const MONITOR_LINES: usize = 50;

const KINDS: [(MidiKind, &str); 3] = [
    (MidiKind::Cc, "CC"),
    (MidiKind::Note, "Note"),
    (MidiKind::PitchBend, "Pitch bend"),
];

const MODES: [MapMode; 6] = [
    MapMode::Absolute,
    MapMode::Relative,
    MapMode::RelativeTwosComplement,
    MapMode::Button,
    MapMode::Toggle,
    MapMode::Absolute14,
];

pub(super) fn mapping_page(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let Some(page) = settings.mapping.as_ref() else {
        return note(
            t,
            "The mapping editor needs MIDI, which is not available (see the terminal for the reason).",
        );
    };
    let mut column = div().flex().flex_col();

    if let Some(confirm) = settings.mapping_confirm {
        column = column.child(confirm_row(t, confirm, cx));
    }

    // Toolbar: the bindings and the map file.
    let learning = page.is_learning();
    column = column.child(choice_row(
        t,
        "Bindings",
        row_of(vec![
            chip(t, "Add", false, cx, |this| {
                this.settings_mut(|s| s.mapping_new())
            }),
            chip(t, "Duplicate", false, cx, |this| {
                this.settings_mut(|s| s.mapping_op(|p| p.duplicate_selected()))
            }),
            chip(t, "Delete", false, cx, |this| {
                this.settings_mut(|s| s.mapping_op(|p| p.delete_selected()))
            }),
            chip(
                t,
                if learning {
                    "Cancel learn"
                } else {
                    "Learn selected"
                },
                learning,
                cx,
                move |this| {
                    this.settings_mut(|s| {
                        if learning {
                            s.mapping_cancel_learn()
                        } else {
                            s.mapping_op(|p| p.learn_selected())
                        }
                    })
                },
            ),
            chip(t, "Undo", false, cx, |this| {
                this.settings_mut(|s| s.mapping_op(|p| p.undo()))
            }),
            chip(t, "Redo", false, cx, |this| {
                this.settings_mut(|s| s.mapping_op(|p| p.redo()))
            }),
            chip(t, "Save", false, cx, |this| {
                this.settings_mut(|s| s.mapping_op(|p| p.save()))
            }),
            chip(t, "Import…", false, cx, |this| {
                this.settings_mut(|s| s.mapping_begin(EntryKind::MapImport, String::new()))
            }),
            chip(t, "Export…", false, cx, |this| {
                this.settings_mut(|s| s.mapping_begin(EntryKind::MapExport, String::new()))
            }),
            chip(t, "Reset…", false, cx, |this| {
                this.settings_mut(|s| s.mapping_confirm = Some(MappingConfirm::Reset))
            }),
        ]),
    ));
    column = column.child(note(
        t,
        &format!(
            "Map: {}. Edits reach the running controller at once; Save writes the file.{}",
            settings.prefs.effective_map_file().display(),
            if page.is_dirty() {
                " Unsaved changes."
            } else {
                ""
            }
        ),
    ));

    // Filter: a text field typed live, plus the target groups.
    let filter = page.filter().clone();
    let mut filter_chips = vec![chip(t, "Filter text…", !filter.text.is_empty(), cx, {
        let text = filter.text.clone();
        move |this| this.settings_mut(|s| s.mapping_begin(EntryKind::MapFilter, text.clone()))
    })];
    filter_chips.push(chip(t, "All groups", filter.group.is_none(), cx, |this| {
        this.settings_mut(|s| s.mapping_set_group(None))
    }));
    for group in page.groups() {
        let lit = filter.group.as_deref() == Some(group.as_str());
        let pick = group.clone();
        filter_chips.push(chip(t, &group, lit, cx, move |this| {
            let pick = pick.clone();
            this.settings_mut(|s| s.mapping_set_group(Some(pick)))
        }));
    }
    column = column.child(choice_row(t, "Filter", row_of(filter_chips)));
    if let Some(field) = entry_field(t, settings, EntryKind::MapFilter, "filter text", cx) {
        column = column.child(field);
    }

    // The binding table.
    let rows = page.rows();
    let selected = page.selected();
    let mut table = div()
        .id("mapping-rows")
        .flex()
        .flex_col()
        .h(px(TABLE_HEIGHT))
        .overflow_y_scroll()
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .mb(px(8.));
    if rows.is_empty() {
        table = table.child(note(
            t,
            "No binding matches. Add one, or learn a control on the controller.",
        ));
    }
    for row in rows {
        let lit = selected == Some(row.index);
        let index = row.index;
        let mark = if row.conflicted { "! " } else { "" };
        table = table.child(
            div()
                .px(px(8.))
                .py(px(3.))
                .bg(paint(if lit { accent_of(t) } else { t.surface_raised }))
                .text_color(paint(if lit { t.on_accent } else { t.text_primary }))
                .text_size(px(t.type_small))
                .child(format!("{mark}{}", row.text))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.settings_mut(|s| s.mapping_select(Some(index)));
                        cx.notify();
                    }),
                ),
        );
    }
    column = column.child(table);

    // The edit form for the selected binding (or a new one).
    match page.form() {
        Some(form) => column = column.child(form_view(t, settings, form, cx)),
        None => column = column.child(note(t, "Select a binding to edit it, or press Add.")),
    }

    // Conflicts: the same control bound twice.
    let conflicts = page.conflicts();
    if !conflicts.is_empty() {
        column = column.child(note(
            t,
            &format!(
                "Conflicts ({}): one control is bound twice. Keep one binding of each pair.",
                conflicts.len()
            ),
        ));
        for conflict in conflicts {
            column = column.child(conflict_row(t, conflict, cx));
        }
    }

    // The MIDI monitor, newest first.
    column = column.child(note(t, "MIDI monitor (newest first)"));
    let lines = page.monitor_lines(MONITOR_LINES);
    let mut monitor = div()
        .id("mapping-monitor")
        .flex()
        .flex_col()
        .h(px(TABLE_HEIGHT * 0.6))
        .overflow_y_scroll()
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .px(px(8.))
        .py(px(4.))
        .text_size(px(t.type_caption))
        .text_color(paint(t.text_secondary));
    if lines.is_empty() {
        monitor = monitor.child("No MIDI event yet. Move a control on the controller.");
    }
    for line in lines {
        monitor = monitor.child(line);
    }
    column = column.child(monitor);

    column.child(status_line(t, &settings.notice))
}

/// The confirm step: Reset, or leaving with unsaved edits. Each names what happens.
fn confirm_row(t: &ThemeTokens, confirm: MappingConfirm, cx: &mut Context<Shell>) -> Div {
    match confirm {
        MappingConfirm::Reset => div()
            .flex()
            .flex_col()
            .mb(px(8.))
            .child(note(
                t,
                "Reset replaces every binding with an empty map. Undo brings them back until you close the tab.",
            ))
            .child(row_of(vec![
                chip(t, "Reset now", true, cx, |this| {
                    this.settings_mut(|s| {
                        s.mapping_confirm = None;
                        s.mapping_op(|p| p.reset_to_default())
                    })
                }),
                chip(t, "Keep the bindings", false, cx, |this| {
                    this.settings_mut(|s| s.mapping_confirm = None)
                }),
            ])),
        MappingConfirm::Leave => div()
            .flex()
            .flex_col()
            .mb(px(8.))
            .child(note(
                t,
                "Unsaved mapping changes. They stay in the running controller but are not in the map file.",
            ))
            .child(row_of(vec![
                chip(t, "Close without saving", true, cx, |this| {
                    this.settings_mut(|s| s.mapping_confirm = Some(MappingConfirm::Leave));
                    this.close_settings();
                }),
                chip(t, "Keep editing", false, cx, |this| {
                    this.settings_mut(|s| s.mapping_confirm = None)
                }),
            ])),
    }
}

/// One conflict with the two ways to resolve it.
fn conflict_row(t: &ThemeTokens, conflict: Conflict, cx: &mut Context<Shell>) -> Div {
    let text = format!(
        "{}: bindings {} and {}",
        conflict.control,
        conflict.first + 1,
        conflict.second + 1
    );
    let first = conflict.clone();
    let second = conflict;
    div()
        .flex()
        .items_center()
        .flex_wrap()
        .mb(px(4.))
        .child(
            div()
                .mr(px(8.))
                .text_size(px(t.type_small))
                .text_color(paint(t.text_primary))
                .child(text),
        )
        .child(chip(t, "Keep first", false, cx, move |this| {
            let c = first.clone();
            this.settings_mut(|s| s.mapping_resolve(&c, Keep::First))
        }))
        .child(chip(t, "Keep second", false, cx, move |this| {
            let c = second.clone();
            this.settings_mut(|s| s.mapping_resolve(&c, Keep::Second))
        }))
}

/// The edit form: kind and mode selectors, the text fields, the live parse error and the buttons.
fn form_view(
    t: &ThemeTokens,
    settings: &SettingsState,
    form: &om_midi::BindingDraft,
    cx: &mut Context<Shell>,
) -> Div {
    let mut column = div().flex().flex_col().mb(px(8.));
    let kinds: Vec<Div> = KINDS
        .iter()
        .map(|(kind, label)| {
            let kind = *kind;
            chip(t, label, form.kind == kind, cx, move |this| {
                this.settings_mut(|s| s.mapping_set_kind(kind))
            })
        })
        .collect();
    column = column.child(choice_row(t, "Message type", row_of(kinds)));
    let modes: Vec<Div> = MODES
        .iter()
        .map(|mode| {
            let mode = *mode;
            chip(t, mode_label(mode), form.mode == mode, cx, move |this| {
                this.settings_mut(|s| s.mapping_set_mode(mode))
            })
        })
        .collect();
    column = column.child(choice_row(t, "Mode", row_of(modes)));

    let values = [
        form.channel.as_str(),
        form.number.as_str(),
        form.min.as_str(),
        form.max.as_str(),
        form.target.as_str(),
    ];
    let fields: Vec<Div> = MapField::ALL
        .iter()
        .zip(values)
        .map(|(field, value)| {
            let field = *field;
            chip(
                t,
                &format!("{}: {}", field.label(), value),
                false,
                cx,
                move |this| this.settings_mut(|s| s.mapping_begin_field(field)),
            )
        })
        .collect();
    column = column.child(choice_row(t, "Fields (click to type)", row_of(fields)));
    for field in MapField::ALL {
        if let Some(entry) = entry_field(t, settings, EntryKind::MapField(field), field.hint(), cx)
        {
            column = column.child(entry);
        }
    }

    let invert = form.invert;
    column = column.child(choice_row(
        t,
        "Options",
        row_of(vec![chip(
            t,
            if invert { "Invert: on" } else { "Invert: off" },
            invert,
            cx,
            |this| this.settings_mut(|s| s.mapping_toggle_invert()),
        )]),
    ));

    // The parse error, shown as the field is typed (Apply refuses the same form).
    match form.to_mapping() {
        Ok(_) => column = column.child(note(t, "The binding is valid.")),
        Err(error) => {
            column = column.child(
                div()
                    .text_color(paint(t.text_primary))
                    .text_size(px(t.type_caption))
                    .mb(px(6.))
                    .child(format!("Not valid yet: {error}")),
            )
        }
    }
    let target = form.target.clone();
    column.child(row_of(vec![
        chip(t, "Apply", false, cx, |this| {
            this.settings_mut(|s| s.mapping_op(|p| p.apply_form()))
        }),
        chip(t, "Learn new for this target", false, cx, move |this| {
            let target = target.clone();
            this.settings_mut(|s| s.mapping_op(|p| p.learn_new(&target)))
        }),
    ]))
}

impl SettingsState {
    /// Runs one editor action. Its result, or the status it leaves, goes to the notice line.
    pub fn mapping_op(&mut self, op: impl FnOnce(&mut MappingEditorPage) -> Result<(), String>) {
        let Some(page) = self.mapping.as_mut() else {
            return self.set_notice("The mapping editor is not available", true);
        };
        let result = op(page);
        let (text, error) = match result {
            Ok(()) => (page.status().to_owned(), false),
            Err(error) => (error, true),
        };
        self.set_notice(text, error);
    }

    /// Adds an empty binding to type into the form.
    pub fn mapping_new(&mut self) {
        if let Some(page) = self.mapping.as_mut() {
            page.new_form();
        }
        self.set_notice("Type the new binding, then Apply", false);
    }

    pub fn mapping_select(&mut self, index: Option<usize>) {
        if let Some(page) = self.mapping.as_mut() {
            page.select(index);
        }
    }

    pub fn mapping_set_kind(&mut self, kind: MidiKind) {
        if let Some(form) = self.mapping.as_mut().and_then(|p| p.form_mut()) {
            form.kind = kind;
        }
    }

    pub fn mapping_set_mode(&mut self, mode: MapMode) {
        if let Some(form) = self.mapping.as_mut().and_then(|p| p.form_mut()) {
            form.mode = mode;
        }
    }

    pub fn mapping_toggle_invert(&mut self) {
        if let Some(form) = self.mapping.as_mut().and_then(|p| p.form_mut()) {
            form.invert = !form.invert;
        }
    }

    /// Opens the text field for `kind` with `text` already in it.
    pub fn mapping_begin(&mut self, kind: EntryKind, text: String) {
        self.entry = Some(TextEntry { kind, text });
    }

    /// Opens the text field for one form field, showing the field's current text.
    pub fn mapping_begin_field(&mut self, field: MapField) {
        let text = self
            .mapping
            .as_ref()
            .and_then(|p| p.form())
            .map(|form| match field {
                MapField::Channel => form.channel.clone(),
                MapField::Number => form.number.clone(),
                MapField::Min => form.min.clone(),
                MapField::Max => form.max.clone(),
                MapField::Target => form.target.clone(),
            })
            .unwrap_or_default();
        self.mapping_begin(EntryKind::MapField(field), text);
    }

    pub fn mapping_set_group(&mut self, group: Option<String>) {
        let Some(page) = self.mapping.as_mut() else {
            return;
        };
        let filter = MappingFilter {
            group,
            ..page.filter().clone()
        };
        page.set_filter(filter);
    }

    /// The filter text as it is typed (the table narrows at once).
    pub fn mapping_filter_live(&mut self) {
        let text = match self.entry.as_ref() {
            Some(entry) if entry.kind == EntryKind::MapFilter => entry.text.clone(),
            _ => return,
        };
        self.set_filter_text(text);
    }

    fn set_filter_text(&mut self, text: String) {
        if let Some(page) = self.mapping.as_mut() {
            let filter = MappingFilter {
                text,
                ..page.filter().clone()
            };
            page.set_filter(filter);
        }
    }

    /// Enter in one of the mapping text fields (the field is already taken off the card). An empty
    /// path to import or export keeps the field open, as the other path fields do.
    pub(super) fn mapping_submit(&mut self, entry: TextEntry) {
        let text = entry.text.trim().to_owned();
        match entry.kind {
            EntryKind::MapFilter => self.set_filter_text(entry.text),
            EntryKind::MapField(field) => {
                if let Some(form) = self.mapping.as_mut().and_then(|p| p.form_mut()) {
                    let value = text;
                    match field {
                        MapField::Channel => form.channel = value,
                        MapField::Number => form.number = value,
                        MapField::Min => form.min = value,
                        MapField::Max => form.max = value,
                        MapField::Target => form.target = value,
                    }
                }
            }
            EntryKind::MapImport | EntryKind::MapExport => {
                if text.is_empty() {
                    let kind = entry.kind;
                    self.entry = Some(TextEntry {
                        kind,
                        text: String::new(),
                    });
                    return self.set_notice("Type a path first", true);
                }
                let path = PathBuf::from(&text);
                if entry.kind == EntryKind::MapImport {
                    self.mapping_op(|p| p.import(&path));
                } else {
                    self.mapping_op(|p| p.export(&path));
                }
            }
            _ => {}
        }
    }

    pub fn mapping_resolve(&mut self, conflict: &Conflict, keep: Keep) {
        self.mapping_op(|p| p.resolve_conflict(conflict, keep));
    }

    pub fn mapping_cancel_learn(&mut self) {
        if let Some(page) = self.mapping.as_mut() {
            page.cancel_learn();
        }
        let text = self.mapping.as_ref().map(|p| p.status().to_owned());
        self.set_notice(text.unwrap_or_default(), false);
    }

    /// Called each frame: a learn that captured a control selects its binding and says so.
    pub fn mapping_tick(&mut self) {
        let Some(page) = self.mapping.as_mut() else {
            return;
        };
        if page.tick().is_some() {
            let text = page.status().to_owned();
            self.set_notice(text, false);
        }
    }

    /// True when the overlay may close. With unsaved edits the first close asks (the confirm
    /// step); the second, from that step, goes ahead.
    pub fn mapping_may_close(&mut self) -> bool {
        let dirty = self
            .mapping
            .as_ref()
            .is_some_and(MappingEditorPage::is_dirty);
        if !dirty || self.mapping_confirm == Some(MappingConfirm::Leave) {
            self.mapping_confirm = None;
            return true;
        }
        self.mapping_confirm = Some(MappingConfirm::Leave);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;
    use om_midi::MidiMapper;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn state_with_editor() -> SettingsState {
        let mut s = state(None, Rc::new(RefCell::new(0)));
        s.mapping = Some(MappingEditorPage::new(
            Arc::new(MidiMapper::new()),
            Some(std::env::temp_dir().join("om-mapping-tab-test.json")),
        ));
        s
    }

    #[test]
    fn a_bad_form_field_is_refused_with_the_parse_error_shown() {
        let mut s = state_with_editor();
        s.mapping_new();
        s.mapping_begin_field(MapField::Channel);
        s.entry.as_mut().unwrap().text = "99".into();
        s.submit_entry();
        let form = s.mapping.as_ref().unwrap().form().unwrap();
        assert_eq!(form.channel, "99");
        assert!(
            form.to_mapping().is_err(),
            "the parse error is what the page shows"
        );
        s.mapping_op(|p| p.apply_form());
        assert!(s.notice.error, "Apply reports the refusal");
        assert!(s.mapping.as_ref().unwrap().rows().is_empty());
    }

    #[test]
    fn filter_text_narrows_the_rows_as_it_is_typed() {
        let mut s = state_with_editor();
        s.mapping_new();
        s.mapping_begin_field(MapField::Target);
        s.entry.as_mut().unwrap().text = "mixer.ch0.fader".into();
        s.submit_entry();
        s.mapping_op(|p| p.apply_form());
        assert_eq!(s.mapping.as_ref().unwrap().rows().len(), 1);
        s.mapping_begin(EntryKind::MapFilter, String::new());
        s.entry.as_mut().unwrap().text = "deck".into();
        s.mapping_filter_live();
        assert_eq!(s.mapping.as_ref().unwrap().rows().len(), 0);
        s.mapping_set_group(None);
    }

    #[test]
    fn leaving_with_unsaved_edits_asks_first() {
        let mut s = state_with_editor();
        s.mapping_new();
        s.mapping_begin_field(MapField::Target);
        s.entry.as_mut().unwrap().text = "mixer.ch0.fader".into();
        s.submit_entry();
        s.mapping_op(|p| p.apply_form());
        assert!(!s.mapping_may_close(), "first close asks");
        assert_eq!(s.mapping_confirm, Some(MappingConfirm::Leave));
        assert!(s.mapping_may_close(), "the second close goes ahead");
    }
}
