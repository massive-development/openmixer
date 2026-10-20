//! The stems section of the Library page: the stem model's state and licence (with the download
//! and cancel buttons), the cache size with Clear, the cache cap (a text field) and the
//! auto-prepare toggle. Every action goes through [`StemsSettingsPage`], so the download starts only
//! after the licence button is pressed.

use super::chips::*;
use super::*;
use crate::app::Shell;
use om_core::jobs::JobHandle;
use om_ui::settings::{format_gib, SettingsResult, StemsSettingsPage};
use om_ui::stems::{ModelState, StemJob};

/// Stands in for the stems host when the library did not open: every action says so.
pub(super) struct NoStems;

impl StemHost for NoStems {
    fn model_state(&self) -> ModelState {
        ModelState::NotInstalled("the library is not available".into())
    }
    fn licence(&self) -> Option<(String, String)> {
        None
    }
    fn accept_and_download(&mut self) -> SettingsResult<()> {
        Err(SettingsError::StemsUnavailable)
    }
    fn cancel_download(&mut self) {}
    fn separator(&self) -> (String, i32) {
        ("none".to_owned(), 0)
    }
    fn cache_bytes(&self) -> u64 {
        0
    }
    fn clear_cache(&mut self) -> SettingsResult<usize> {
        Ok(0)
    }
    fn set_max_cache_bytes(&mut self, _bytes: u64) {}
    fn auto_prepare(&self) -> bool {
        false
    }
    fn set_auto_prepare(&mut self, _on: bool) {}
    fn submit(&mut self, _job: StemJob) -> SettingsResult<JobHandle> {
        Err(SettingsError::StemsUnavailable)
    }
}

pub(super) fn stems_section(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let Some(host) = settings.stems.as_deref() else {
        return choice_row(
            t,
            "Stem model",
            note(t, "Stems are not available: the library did not open."),
        );
    };
    let state = host.model_state();
    let licence = host.licence();

    let mut model = div().flex().flex_col().child(note(
        t,
        "Stems are prepared for selected tracks (Prepare stems) or, with auto-prepare on, for \
         tracks analysed after they were added. Loading a track never starts separation.",
    ));
    model = model.child(note(t, &state.line()));
    if let Some((name, text)) = licence.as_ref() {
        if !state.ready() {
            model = model.child(licence_block(t, name, text));
        }
    }
    let mut buttons = Vec::new();
    if matches!(state, ModelState::Downloading(_)) {
        buttons.push(chip(t, "Cancel download", false, cx, |this| {
            this.settings_mut(|s| s.stems_cancel())
        }));
    } else if !state.ready() && licence.is_some() {
        buttons.push(chip(t, "Accept licence and download", false, cx, |this| {
            this.settings_mut(|s| s.stems_accept())
        }));
    }
    if !buttons.is_empty() {
        model = model.child(row_of(buttons));
    }

    let mut cache = div().flex().flex_col().child(note(
        t,
        &format!(
            "{} of {} GB used",
            format_gib(settings.stems_bytes),
            settings.prefs.stems.max_cache_gb
        ),
    ));
    if settings.stems_confirm_clear {
        cache = cache
            .child(note(
                t,
                "Remove every prepared stem set? Running prepare jobs are cancelled.",
            ))
            .child(row_of(vec![
                chip(t, "Remove stems", false, cx, |this| {
                    this.settings_mut(|s| s.stems_clear_confirmed())
                }),
                chip(t, "Keep them", false, cx, |this| {
                    this.settings_mut(|s| s.stems_clear_cancelled())
                }),
            ]));
    } else {
        cache = cache.child(row_of(vec![chip(
            t,
            "Clear stem cache…",
            false,
            cx,
            |this| this.settings_mut(|s| s.stems_ask_clear()),
        )]));
    }

    let cap_gb = settings.prefs.stems.max_cache_gb;
    let mut cap = div().flex().flex_col().child(row_of(vec![chip(
        t,
        &format!("{cap_gb} GB…"),
        false,
        cx,
        |this| this.settings_mut(|s| s.begin_entry(EntryKind::MaxCacheGb)),
    )]));
    if let Some(field) = entry_field(t, settings, EntryKind::MaxCacheGb, "cache size in GB", cx) {
        cap = cap.child(field);
    }

    let auto = settings.prefs.stems.auto_prepare;
    let auto_chip = chip(t, if auto { "On" } else { "Off" }, auto, cx, move |this| {
        this.settings_mut(|s| s.stems_set_auto_prepare(!auto))
    });

    div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Stem model", model))
        .child(choice_row(t, "Stem cache", cache))
        .child(choice_row(t, "Max cache", cap))
        .child(choice_row(
            t,
            "Auto-prepare new tracks",
            row_of(vec![auto_chip]),
        ))
}

/// The licence text in a box that scrolls, so a long licence fits the card.
fn licence_block(t: &ThemeTokens, name: &str, text: &str) -> Div {
    let mut body = div()
        .flex()
        .flex_col()
        .p(px(8.))
        .text_color(paint(t.text_secondary))
        .text_size(px(t.type_caption))
        .child(
            div()
                .text_color(paint(t.text_primary))
                .child(name.to_owned()),
        );
    for paragraph in text.split("\n\n") {
        body = body.child(div().mt(px(6.)).child(paragraph.replace('\n', " ")));
    }
    div().mb(px(8.)).child(
        div()
            .id("stems-licence")
            .max_h(px(160.))
            .overflow_y_scroll()
            .border_1()
            .border_color(paint(t.border))
            .rounded(px(t.control_radius))
            .child(body),
    )
}

impl Shell {
    /// Opens the settings on the Library tab, where the stem licence is (see
    /// [`SettingsState::show_library_tab`]). A window without settings has nowhere to show it.
    pub(crate) fn show_stems_licence(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.show_library_tab();
        }
    }
}

impl SettingsState {
    /// Runs `f` on the stems page over the host (or the stand-in), with the preferences.
    fn with_stems_page<T>(
        &mut self,
        f: impl FnOnce(&mut StemsSettingsPage<'_>) -> SettingsResult<T>,
    ) -> SettingsResult<T> {
        let mut none = NoStems;
        let host: &mut dyn StemHost = match self.stems.as_deref_mut() {
            Some(host) => host,
            None => &mut none,
        };
        let mut page = StemsSettingsPage::new(&mut self.prefs, host);
        f(&mut page)
    }

    fn stems_line(&self) -> String {
        self.stems
            .as_deref()
            .map_or_else(String::new, |host| host.model_state().line())
    }

    fn stems_refresh(&mut self) {
        if let Some(host) = self.stems.as_deref() {
            self.stems_bytes = host.cache_bytes();
        }
    }

    /// The licence button: the user has accepted, so the download starts.
    pub fn stems_accept(&mut self) {
        match self.with_stems_page(|p| p.accept_and_download()) {
            Ok(()) => {
                let line = self.stems_line();
                self.set_notice(line, false);
            }
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    pub fn stems_cancel(&mut self) {
        self.with_stems_page(|p| {
            p.cancel_download();
            Ok(())
        })
        .ok();
        self.set_notice("Download cancelled", false);
    }

    pub fn stems_set_auto_prepare(&mut self, on: bool) {
        if matches!(
            self.with_stems_page(|p| Ok(p.set_auto_prepare(on))),
            Ok(true)
        ) {
            self.changed();
        }
    }

    pub fn stems_ask_clear(&mut self) {
        self.stems_confirm_clear = true;
    }

    pub fn stems_clear_cancelled(&mut self) {
        self.stems_confirm_clear = false;
    }

    pub fn stems_clear_confirmed(&mut self) {
        self.stems_confirm_clear = false;
        match self.with_stems_page(|p| p.clear_cache()) {
            Ok(removed) => self.set_notice(format!("Removed {removed} stem set(s)"), false),
            Err(error) => self.set_notice(error.to_string(), true),
        }
        self.stems_refresh();
    }

    /// The cap field: a whole number of GB. Anything else keeps the field open.
    pub(super) fn stems_submit_max_cache(&mut self, text: &str) {
        match text.parse::<i32>() {
            Ok(gb) => {
                if matches!(
                    self.with_stems_page(|p| Ok(p.set_max_cache_gb(gb))),
                    Ok(true)
                ) {
                    self.changed();
                }
                let shown = self.prefs.stems.max_cache_gb;
                self.set_notice(format!("Cache cap is {shown} GB"), false);
            }
            Err(_) => {
                self.entry = Some(TextEntry {
                    kind: EntryKind::MaxCacheGb,
                    text: text.to_owned(),
                });
                self.set_notice("Type a whole number of GB (1 to 1000)", true);
            }
        }
    }

    /// Opens the settings on the Library tab (the stems licence section), for an action that needs
    /// the model first.
    pub(crate) fn show_library_tab(&mut self) {
        if let Some(index) = self.panel.tabs().iter().position(|t| t == "Library") {
            self.panel.show(index);
        }
        self.open = true;
        self.focus_pending = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// A host whose model is not installed; counts download requests.
    #[derive(Default)]
    struct Model {
        downloads: Rc<Cell<usize>>,
        cap: Rc<Cell<u64>>,
    }

    impl StemHost for Model {
        fn model_state(&self) -> ModelState {
            ModelState::LicenceNotAccepted
        }
        fn licence(&self) -> Option<(String, String)> {
            Some(("MIT".into(), "Licence text".into()))
        }
        fn accept_and_download(&mut self) -> SettingsResult<()> {
            self.downloads.set(self.downloads.get() + 1);
            Ok(())
        }
        fn cancel_download(&mut self) {}
        fn separator(&self) -> (String, i32) {
            ("htdemucs".into(), 1)
        }
        fn cache_bytes(&self) -> u64 {
            0
        }
        fn clear_cache(&mut self) -> SettingsResult<usize> {
            Ok(0)
        }
        fn set_max_cache_bytes(&mut self, bytes: u64) {
            self.cap.set(bytes);
        }
        fn auto_prepare(&self) -> bool {
            false
        }
        fn set_auto_prepare(&mut self, _on: bool) {}
        fn submit(&mut self, _job: StemJob) -> SettingsResult<JobHandle> {
            Err(SettingsError::StemsUnavailable)
        }
    }

    #[test]
    fn the_licence_button_is_the_only_way_to_start_a_download() {
        let downloads = Rc::new(Cell::new(0));
        let mut o = options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );
        o.stems = Some(Box::new(Model {
            downloads: Rc::clone(&downloads),
            cap: Rc::new(Cell::new(0)),
        }));
        let mut s = SettingsState::new(o);
        s.stems_set_auto_prepare(true);
        assert_eq!(downloads.get(), 0, "changing a preference fetches nothing");
        s.stems_accept();
        assert_eq!(downloads.get(), 1);
    }

    #[test]
    fn the_cap_field_takes_whole_gb_and_keeps_the_field_open_otherwise() {
        let cap = Rc::new(Cell::new(0));
        let mut o = options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );
        o.stems = Some(Box::new(Model {
            downloads: Rc::new(Cell::new(0)),
            cap: Rc::clone(&cap),
        }));
        let mut s = SettingsState::new(o);
        s.stems_submit_max_cache("abc");
        assert!(s.notice.error);
        assert!(s.entry.is_some(), "the field stays open");
        s.stems_submit_max_cache("12");
        assert_eq!(s.prefs.stems.max_cache_gb, 12);
        assert_eq!(cap.get(), 12 * 1024 * 1024 * 1024);
    }

    #[test]
    fn clearing_asks_first_and_the_library_tab_opens_for_the_licence() {
        let mut s = state(None, Rc::new(RefCell::new(0)));
        s.stems_ask_clear();
        assert!(s.stems_confirm_clear);
        s.stems_clear_cancelled();
        assert!(!s.stems_confirm_clear);
        s.show_library_tab();
        assert!(s.open);
        assert_eq!(s.panel.tabs()[s.panel.current()], "Library");
    }
}
