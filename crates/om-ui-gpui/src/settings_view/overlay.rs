//! The overlay around the pages: header button, keys, the text field's typed input and the card.

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
use crate::text_entry::{apply_key, EntryKey};
use std::time::Instant;

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

    /// A key while the overlay is open: the text field takes the typed text (Enter applies,
    /// Escape cancels it); otherwise Escape closes the overlay.
    pub(crate) fn settings_key(&mut self, key: &str, typed: Option<&str>) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if !settings.open {
            return;
        }
        if let Some(entry) = settings.entry.as_mut() {
            let kind = entry.kind;
            match apply_key(&mut entry.text, key, typed) {
                EntryKey::Submit => settings.submit_entry(),
                EntryKey::Cancel => settings.cancel_entry(),
                // The mapping filter narrows the table as it is typed.
                EntryKey::Edited if kind == EntryKind::MapFilter => settings.mapping_filter_live(),
                EntryKey::Edited | EntryKey::Ignored => {}
            }
        } else if key == "escape" {
            self.close_settings();
        }
    }

    /// The overlay for a window of `area` size, or `None` while it is closed.
    pub(crate) fn settings_overlay(
        &mut self,
        area: Rect,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> Option<Div> {
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
            focus.focus(window);
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
        let mut tabs = div().flex().flex_wrap().mb(px(10.));
        for (index, title) in settings.panel.tabs().to_vec().into_iter().enumerate() {
            tabs = tabs.child(chip(&t, &title, index == current, cx, move |this| {
                this.settings_mut(|s| {
                    s.panel.show(index);
                    s.refresh_lists();
                });
            }));
        }
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
        let status = status_line(&t, &settings.notice);
        let close = chip(&t, "Close", false, cx, |this| this.close_settings());

        let click_focus = focus.clone();
        let card = place(Rect::new(card_x, card_y, w, h))
            .flex()
            .flex_col()
            .p(px(14.))
            .bg(paint(t.popup))
            .border_1()
            .border_color(paint(t.border))
            .rounded(px(t.control_radius))
            .text_color(paint(t.text_primary))
            .text_size(px(t.type_small))
            .overflow_hidden()
            .track_focus(&focus)
            .child(
                div()
                    .text_size(px(t.type_heading))
                    .mb(px(8.))
                    .child("Settings"),
            )
            .child(tabs)
            // The page scrolls: the Library page with the stems section is taller than the card.
            .child(
                div()
                    .id("settings-page")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(body),
            )
            .child(status)
            .child(div().flex().justify_end().child(close))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                let keys = &ev.keystroke;
                if keys.modifiers.control || keys.modifiers.platform {
                    return;
                }
                this.settings_key(&keys.key, keys.key_char.as_deref());
                cx.notify();
            }))
            // Clicks on the card do not reach the scrim below, and the card takes the keys.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, window, cx| {
                    click_focus.focus(window);
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
