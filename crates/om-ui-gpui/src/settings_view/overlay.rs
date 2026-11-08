//! The overlay around the pages: header button, keys, the text field's input and the card. The
//! tab strip is a `gpui-component` tab bar, the text field an input, the confirmations alert
//! dialogs and what an action reports a toast.

use super::appearance::*;
use super::audio::*;
use super::automix::*;
use super::chips::*;
use super::library::*;
use super::mapping::*;
use super::midi::*;
use super::plugins::*;
use super::recording::*;
use super::*;
use crate::paint::skin;
use gpui::{SharedString, WeakEntity};
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::WindowExt as _;
use om_ui::settings::ClapStatus;
use om_ui::style::LookAndFeel;
use std::time::Instant;

/// A question the overlay asks in a dialog before an action that would lose work. Each is the
/// state flag of a settings page (`stems_confirm_clear`, `mapping_confirm`) shown as a dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    ClearStems,
    MappingReset,
    MappingLeave,
    /// First run: may the standard CLAP folders be scanned? (Scanning runs plugin code.)
    ClapFirstRun,
    /// A CLAP plugin is about to load for the first time: it runs in this process.
    ClapFirstLoad,
}

/// How a dialog ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Ok,
    Cancel,
    /// Left without pressing either button (Escape, a click outside).
    Dismissed,
}

impl Ask {
    fn title(self) -> &'static str {
        match self {
            Ask::ClearStems => "Clear the stem cache?",
            Ask::MappingReset => "Reset the mapping?",
            Ask::MappingLeave => "Unsaved mapping changes",
            Ask::ClapFirstRun => "Look for CLAP plugins in the standard folders?",
            Ask::ClapFirstLoad => "Load CLAP plugins?",
        }
    }

    pub(crate) fn body(self, clap: &ClapStatus) -> String {
        match self {
            Ask::ClearStems => {
                "Remove every prepared stem set? Running prepare jobs are cancelled.".to_owned()
            }
            Ask::MappingReset => {
                "Reset replaces every binding with an empty map. Undo brings them back until you \
                 close the tab."
                    .to_owned()
            }
            Ask::MappingLeave => {
                "They stay in the running controller but are not in the map file.".to_owned()
            }
            Ask::ClapFirstRun => clap_first_run_text(clap),
            Ask::ClapFirstLoad => clap_first_load_text(clap),
        }
    }

    fn ok(self) -> &'static str {
        match self {
            Ask::ClearStems => "Remove stems",
            Ask::MappingReset => "Reset now",
            Ask::MappingLeave => "Close without saving",
            Ask::ClapFirstRun => "Scan these folders",
            Ask::ClapFirstLoad => "Load the plugins",
        }
    }

    fn cancel(self) -> &'static str {
        match self {
            Ask::ClearStems => "Keep them",
            Ask::MappingReset => "Keep the bindings",
            Ask::MappingLeave => "Keep editing",
            Ask::ClapFirstRun => "Do not scan",
            Ask::ClapFirstLoad => "Not now",
        }
    }
}

/// The first-run question's text. Scanning opens every plugin file in the folders, and opening a
/// plugin file runs its code, which is why this is asked and not assumed.
pub(crate) fn clap_first_run_text(clap: &ClapStatus) -> String {
    let folders = if clap.standard_folders.is_empty() {
        "the standard CLAP folders".to_owned()
    } else {
        clap.standard_folders.join(" and ")
    };
    format!(
        "OpenMixer can look for CLAP plugins in {folders}. Looking opens each plugin file, and \
         opening a plugin file runs code written by someone else. Plugins in your OpenMixer \
         plugin folder are looked at either way. You can change this later on the Plugins tab."
    )
}

/// The first-load warning's text (ADR 0007, decision 1): the plugin runs inside this process.
pub(crate) fn clap_first_load_text(clap: &ClapStatus) -> String {
    let files = match clap.waiting {
        1 => "1 CLAP plugin file is".to_owned(),
        n => format!("{n} CLAP plugin files are"),
    };
    format!(
        "{files} waiting. A CLAP plugin is native code made by someone else. It runs inside \
         OpenMixer with the same access to your files as OpenMixer has, and OpenMixer cannot \
         contain it: a plugin that crashes takes the whole app, and your set, down with it. Load \
         only plugins you trust."
    )
}

impl Shell {
    /// Runs `f` on the settings when the window has them.
    pub(crate) fn settings_mut(&mut self, f: impl FnOnce(&mut SettingsState)) {
        if let Some(settings) = self.settings.as_mut() {
            f(settings);
        }
    }

    /// Takes the window's theme from the settings registry (after a theme or plugin change).
    pub(super) fn refresh_theme(&mut self) {
        if let Some(settings) = self.settings.as_ref() {
            self.theme = settings.themes.current().clone();
            self.theme_dirty = true;
        }
    }

    pub(crate) fn open_settings(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.refresh_lists();
            settings.open = true;
            settings.focus_pending = true;
        }
    }

    pub(crate) fn close_settings(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            // Unsaved mapping edits ask first: the first close shows the confirm step.
            if !settings.mapping_may_close() {
                return;
            }
            settings.open = false;
            settings.entry = None;
        }
    }

    /// A key while the overlay is open that the focused control did not take: Escape cancels the
    /// open text field, else closes the overlay. Typed text and Enter belong to the field's input.
    pub(crate) fn settings_key(&mut self, key: &str) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if !settings.open || key != "escape" {
            return;
        }
        if settings.entry.is_some() {
            settings.cancel_entry();
        } else {
            self.close_settings();
        }
    }

    /// Brings the components the overlay owns in step with its state: the text field's input, the
    /// confirmation dialog and the toast for the last notice. Runs every frame, open or not.
    fn sync_settings_chrome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_entry_input(window, cx);
        self.sync_confirm(window, cx);
        self.push_notice(window, cx);
    }

    /// Makes the input for the open text field (and drops it when the field closes). Typing edits
    /// the entry, and Enter applies it.
    fn sync_entry_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        let wanted = settings.entry.as_ref().map(|e| (e.kind, e.text.clone()));
        let shown = settings.entry_input.as_ref().map(|(kind, _)| *kind);
        match (wanted, shown) {
            (Some((kind, text)), shown) if shown != Some(kind) => {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(text));
                cx.subscribe_in(
                    &input,
                    window,
                    move |this, input, event: &InputEvent, _, cx| {
                        match event {
                            InputEvent::Change => {
                                let value = input.read(cx).value().to_string();
                                this.settings_mut(|s| {
                                    if let Some(entry) = s.entry.as_mut() {
                                        entry.text = value;
                                    }
                                    // The mapping filter narrows the table as it is typed.
                                    if kind == EntryKind::MapFilter {
                                        s.mapping_filter_live();
                                    }
                                });
                            }
                            InputEvent::PressEnter { .. } => {
                                this.settings_mut(|s| s.submit_entry());
                            }
                            InputEvent::Focus | InputEvent::Blur => {}
                        }
                        cx.notify();
                    },
                )
                .detach();
                input.update(cx, |input, cx| input.focus(window, cx));
                settings.entry_input = Some((kind, input));
            }
            (None, Some(_)) => {
                settings.entry_input = None;
                // The keyboard goes back to the card, so Escape closes it again.
                if let Some(focus) = self.settings_focus.clone() {
                    focus.focus(window, cx);
                }
            }
            _ => {}
        }
    }

    /// Opens the dialog for the confirmation the settings are waiting on, and closes it when the
    /// settings stop waiting.
    fn sync_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        let wanted = if settings.stems_confirm_clear {
            Some(Ask::ClearStems)
        } else if let Some(confirm) = settings.mapping_confirm {
            Some(match confirm {
                MappingConfirm::Reset => Ask::MappingReset,
                MappingConfirm::Leave => Ask::MappingLeave,
            })
        } else {
            settings.clap_ask_due()
        };
        if wanted == settings.ask_shown {
            return;
        }
        if settings.ask_shown.take().is_some() {
            window.close_dialog(cx);
        }
        let Some(ask) = wanted else {
            return;
        };
        settings.ask_shown = Some(ask);
        let body = ask.body(&settings.lists.clap);
        let shell: WeakEntity<Shell> = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (on_ok, on_cancel, on_close) = (shell.clone(), shell.clone(), shell.clone());
            let resolve = move |shell: &WeakEntity<Shell>, answer: Answer, cx: &mut gpui::App| {
                shell
                    .update(cx, |this, cx| {
                        this.resolve_ask(ask, answer);
                        cx.notify();
                    })
                    .ok();
            };
            alert
                .confirm()
                .title(SharedString::from(ask.title()))
                .description(SharedString::from(body.clone()))
                .ok_text(ask.ok())
                .cancel_text(ask.cancel())
                .on_ok(move |_, _, cx| {
                    resolve(&on_ok, Answer::Ok, cx);
                    true
                })
                .on_cancel(move |_, _, cx| {
                    resolve(&on_cancel, Answer::Cancel, cx);
                    true
                })
                // Any other way out of the dialog is not an answer: things stay as they were (the
                // CLAP questions are asked again on the next start).
                .on_close(move |_, _, cx| resolve(&on_close, Answer::Dismissed, cx))
        });
    }

    /// The dialog for `ask` was answered (`ok`) or dismissed. A no-op when it was already.
    pub(crate) fn resolve_ask(&mut self, ask: Ask, answer: Answer) {
        if self.settings.as_ref().and_then(|s| s.ask_shown) != Some(ask) {
            return;
        }
        self.settings_mut(|s| s.ask_shown = None);
        let ok = answer == Answer::Ok;
        match (ask, ok) {
            (Ask::ClapFirstRun | Ask::ClapFirstLoad, _) => {
                self.settings_mut(|s| s.clap_answered(ask, answer));
                self.refresh_theme();
            }
            (Ask::ClearStems, true) => self.settings_mut(|s| s.stems_clear_confirmed()),
            (Ask::ClearStems, false) => self.settings_mut(|s| s.stems_clear_cancelled()),
            (Ask::MappingReset, true) => self.settings_mut(|s| {
                s.mapping_confirm = None;
                s.mapping_op(|p| p.reset_to_default());
            }),
            (Ask::MappingLeave, true) => {
                self.settings_mut(|s| s.mapping_confirm = Some(MappingConfirm::Leave));
                self.close_settings();
            }
            (Ask::MappingReset | Ask::MappingLeave, false) => {
                self.settings_mut(|s| s.mapping_confirm = None);
            }
        }
    }

    /// Shows the last action's outcome as a toast, once.
    fn push_notice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if settings.toasted_serial == settings.notice_serial() {
            return;
        }
        settings.toasted_serial = settings.notice_serial();
        let notice = settings.notice.clone();
        if notice.text.is_empty() {
            return;
        }
        let toast = if notice.error {
            Notification::error(notice.text)
        } else {
            Notification::info(notice.text)
        };
        window.push_notification(toast, cx);
    }

    /// The overlay for a window of `area` size, or `None` while it is closed.
    pub(crate) fn settings_overlay(
        &mut self,
        area: Rect,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Option<Div> {
        self.sync_settings_chrome(window, cx);
        let t = self.theme.clone();
        let focus: FocusHandle = self
            .settings_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let settings = self.settings.as_mut()?;
        if !settings.open {
            return None;
        }
        if std::mem::take(&mut settings.focus_pending) {
            focus.focus(window, cx);
        }
        settings.poll_learn();
        settings.poll_midi_inputs(Instant::now());
        settings.mapping_tick();
        let w = CARD_WIDTH.min(area.w - 2 * CARD_MARGIN).max(0);
        // The Appearance page is short: its card fits the content instead of showing empty space.
        let wanted =
            if settings.panel.page_kind(settings.panel.current()) == Some(PageKind::Appearance) {
                CARD_HEIGHT * 5 / 9
            } else {
                CARD_HEIGHT
            };
        let h = wanted.min(area.h - 2 * CARD_MARGIN).max(0);
        let card_x = area.x + (area.w - w) / 2;
        let card_y = area.y + (area.h - h) / 2;

        // Tabs, then the page of the tab shown.
        let current = settings.panel.current();
        let tabs = TabBar::new("settings-tabs")
            .selected_index(current)
            .children(
                settings
                    .panel
                    .tabs()
                    .iter()
                    .map(|title| Tab::new().label(title.clone())),
            )
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                let index = *index;
                this.settings_mut(|s| {
                    s.panel.show(index);
                    s.refresh_lists();
                });
                cx.notify();
            }));
        let body = match settings.panel.page_kind(current) {
            Some(PageKind::Audio) => match settings.audio_view() {
                Some(view) => audio_page(&t, &view, cx),
                None => note(&t, "No audio device model."),
            },
            Some(PageKind::Appearance) => appearance_page(&t, settings, cx),
            Some(PageKind::Midi) => midi_page(&t, settings, cx),
            Some(PageKind::Library) => library_page(&t, settings, cx),
            Some(PageKind::Automix) => automix_page(&t, settings, cx),
            Some(PageKind::Plugins) => plugins_page(&t, settings, cx),
            Some(PageKind::Recording) => recording_page(&t, settings, cx),
            Some(PageKind::Mapping) => mapping_page(&t, settings, cx),
            None => note(&t, "This page is added by a plugin."),
        };
        let close = chip(&t, "Close", false, cx, |this| this.close_settings());

        let click_focus = focus.clone();
        let popup = LookAndFeel::from_tokens(&t).popup;
        let card = skin(
            place(Rect::new(card_x, card_y, w, h))
                .flex()
                .flex_col()
                .p(px(14.))
                .overflow_hidden(),
            &popup,
        )
        .track_focus(&focus)
        .child(
            div()
                .text_size(px(t.type_heading))
                .mb(px(8.))
                .child("Settings"),
        )
        .child(div().mb(px(10.)).child(tabs))
        // The page scrolls: the Library page with the stems section is taller than the card.
        .child(
            div()
                .id("settings-page")
                .flex_1()
                .overflow_y_scroll()
                .child(body),
        )
        .child(div().flex().justify_end().child(close))
        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
            let keys = &ev.keystroke;
            if keys.modifiers.control || keys.modifiers.platform {
                return;
            }
            this.settings_key(&keys.key);
            cx.notify();
        }))
        // Clicks on the card do not reach the scrim below, and the card takes the keys.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _: &MouseDownEvent, window, cx| {
                click_focus.focus(window, cx);
                cx.stop_propagation();
            }),
        );

        let scrim = place(area)
            .bg(crate::paint::scrim(t.settings_dim_alpha))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.close_settings();
                    cx.notify();
                }),
            )
            .child(card);
        Some(scrim)
    }
}

impl SettingsState {
    pub fn begin_entry(&mut self, kind: EntryKind) {
        self.entry = Some(TextEntry {
            kind,
            text: String::new(),
        });
    }

    pub fn cancel_entry(&mut self) {
        self.entry = None;
    }

    pub fn type_text(&mut self, text: &str) {
        if let Some(entry) = self.entry.as_mut() {
            entry.text.push_str(text);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(entry) = self.entry.as_mut() {
            entry.text.pop();
        }
    }

    /// Enter: does what the field is for with the typed path. An empty path keeps the field open.
    pub fn submit_entry(&mut self) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        if matches!(
            entry.kind,
            EntryKind::MapFilter
                | EntryKind::MapImport
                | EntryKind::MapExport
                | EntryKind::MapField(_)
        ) {
            return self.mapping_submit(entry);
        }
        let text = entry.text.trim().to_owned();
        if entry.kind == EntryKind::MaxCacheGb {
            self.stems_submit_max_cache(&text);
            return;
        }
        if entry.kind == EntryKind::RecordFolder && !text.is_empty() {
            self.record_set_folder(&text);
            return;
        }
        if text.is_empty() {
            self.entry = Some(entry);
            self.set_notice("Type a path first", true);
            return;
        }
        let path = PathBuf::from(text);
        match entry.kind {
            EntryKind::WatchFolder => self.library_add_folder(path),
            EntryKind::ImportXml => self.library_import(path),
            EntryKind::MapFile => self.midi_load_map(path),
            // Handled above, before the path is made.
            EntryKind::MaxCacheGb
            | EntryKind::RecordFolder
            | EntryKind::MapFilter
            | EntryKind::MapImport
            | EntryKind::MapExport
            | EntryKind::MapField(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;

    use std::cell::Cell;

    use std::rc::Rc;

    #[test]
    fn start_tab_opens_the_overlay_on_the_named_tab() {
        let mut o = options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );
        o.start_tab = Some("library".to_owned());
        let s = SettingsState::new(o);
        assert!(s.open);
        assert_eq!(s.panel.tabs()[s.panel.current()], "Library");
    }

    #[test]
    fn an_empty_path_keeps_the_field_open_and_cancel_closes_it() {
        let mut s = SettingsState::new(options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        ));
        s.begin_entry(EntryKind::WatchFolder);
        s.submit_entry();
        assert!(s.entry.is_some());
        assert!(s.notice.error);
        s.cancel_entry();
        assert!(s.entry.is_none());
    }
}
