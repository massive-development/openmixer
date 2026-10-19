//! The settings overlay (Todo 29): a card with the five tabs over the window, opened by the Settings
//! button in the header and closed by its Close button or a click on the scrim. Port of the drawing
//! in `src/ui/SettingsPanel.cpp` and the pages. The logic (what each control does, what is stored)
//! lives in om-ui's `settings` module; this file reads it and forwards clicks to it.
//!
//! Pages: Audio and Appearance are live. MIDI, Library and Plugins show their stored state and say
//! what is not ported yet (`om-midi`, `om-library`, `om-plugins`).

use crate::app::Shell;
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent};
use om_ui::geometry::Rect;
use om_ui::preferences::Preferences;
use om_ui::settings::{
    stats_text, AppearanceSettingsPage, AudioSettingsModel, AudioSettingsPage, AudioStats,
    PageKind, SettingsPanel, SettingsResult,
};
use om_ui::theme::{ThemeRegistry, ThemeTokens};
use std::sync::Arc;

/// The audio readout, read on the UI thread each frame.
pub type AudioStatsFn = Arc<dyn Fn() -> AudioStats + Send + Sync>;

/// Width and height the card wants; it shrinks to fit the window.
const CARD_WIDTH: i32 = 720;
const CARD_HEIGHT: i32 = 540;
const CARD_MARGIN: i32 = 24;
/// Height of the header strip above the layout, with the Settings button.
pub const HEADER_HEIGHT: i32 = 32;

/// What the binary hands the window to run the settings.
pub struct SettingsOptions {
    pub prefs: Preferences,
    pub themes: ThemeRegistry,
    /// `None` leaves the Audio tab out (the C++ panel does the same without a model).
    pub audio: Option<Box<dyn AudioSettingsModel>>,
    pub audio_stats: Option<AudioStatsFn>,
    /// Called after every edit that changed something: save and apply the preferences.
    pub on_changed: Box<dyn FnMut(&Preferences)>,
}

/// The overlay's state. Edits go to `prefs` and `audio`, then `on_changed` is called.
pub struct SettingsState {
    pub panel: SettingsPanel,
    pub open: bool,
    pub prefs: Preferences,
    pub themes: ThemeRegistry,
    audio: Option<Box<dyn AudioSettingsModel>>,
    audio_stats: Option<AudioStatsFn>,
    on_changed: Box<dyn FnMut(&Preferences)>,
    /// Text of the last failed audio change, shown in place of the readout.
    setup_error: String,
}

/// Plain values the Audio page shows, read once per frame.
struct AudioView {
    types: Vec<String>,
    device_type: String,
    devices: Vec<String>,
    device: String,
    rates: Vec<u32>,
    rate: u32,
    buffers: Vec<u32>,
    buffer: u32,
    pairs: Vec<String>,
    master: usize,
    cue: Option<usize>,
    readout: String,
}

impl SettingsState {
    pub fn new(options: SettingsOptions) -> Self {
        let has_audio = options.audio.is_some();
        Self {
            panel: SettingsPanel::new(has_audio),
            open: false,
            prefs: options.prefs,
            themes: options.themes,
            audio: options.audio,
            audio_stats: options.audio_stats,
            on_changed: options.on_changed,
            setup_error: String::new(),
        }
    }

    /// Runs one audio edit. A failure becomes the readout's text; a change calls `on_changed`.
    pub fn edit_audio(&mut self, edit: impl FnOnce(&mut AudioSettingsPage<'_>) -> SettingsResult) {
        let Some(model) = self.audio.as_deref_mut() else {
            return;
        };
        let result = {
            let mut page = AudioSettingsPage::new(&mut self.prefs, model);
            edit(&mut page)
        };
        match result {
            Ok(changed) => {
                self.setup_error.clear();
                if changed {
                    (self.on_changed)(&self.prefs);
                }
            }
            Err(text) => self.setup_error = text,
        }
    }

    /// Stores the theme and selects it in this window's registry (the live switch). `false` when
    /// the id is unknown, in which case nothing changes.
    pub fn pick_theme(&mut self, id: &str) -> bool {
        let result = {
            let mut page = AppearanceSettingsPage::new(&mut self.prefs, &self.themes);
            page.pick(id)
        };
        match result {
            Ok(changed) => {
                self.themes.select(id);
                if changed {
                    (self.on_changed)(&self.prefs);
                }
                true
            }
            Err(_) => false,
        }
    }

    /// The Audio page's values, read through the page logic.
    fn audio_view(&mut self) -> Option<AudioView> {
        let stats = self.audio_stats.as_ref().map(|f| f()).unwrap_or_default();
        let readout = stats_text(&stats, &self.setup_error);
        let model = self.audio.as_deref_mut()?;
        let types = model.device_types();
        let device_type = model.device_type();
        let devices = model.output_devices();
        let device = model.output_device();
        let rates = model.sample_rates();
        let rate = model.sample_rate();
        let buffers = model.buffer_sizes();
        let buffer = model.buffer_size();
        let page = AudioSettingsPage::new(&mut self.prefs, model);
        Some(AudioView {
            types,
            device_type,
            devices,
            device,
            rates,
            rate,
            buffers,
            buffer,
            pairs: page.pair_labels(),
            master: page.audio_prefs().master_pair.max(0) as usize,
            cue: page.shown_cue_pair(),
            readout,
        })
    }
}

/// The accent for lit chips: the theme's first accent colour.
fn accent_of(t: &ThemeTokens) -> om_ui::theme::Colour {
    t.accents.first().copied().unwrap_or(t.surface_raised)
}

/// A flow-layout button: a labelled chip that runs `on_press` on the Shell when clicked.
fn chip(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(px(10.))
        .py(px(4.))
        .mr(px(6.))
        .mb(px(6.))
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(if lit { accent_of(t) } else { t.surface_raised }))
        .text_color(paint(if lit { t.on_accent } else { t.text_primary }))
        .text_size(px(t.type_small))
        .child(label.to_owned())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                on_press(this);
                cx.notify();
            }),
        )
}

/// A labelled row of chips.
fn choice_row(t: &ThemeTokens, title: &str, chips: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .mb(px(10.))
        .child(
            div()
                .text_color(paint(t.text_secondary))
                .text_size(px(t.type_caption))
                .mb(px(4.))
                .child(title.to_owned()),
        )
        .child(div().flex().flex_wrap().child(chips))
}

fn note(t: &ThemeTokens, text: &str) -> Div {
    div()
        .text_color(paint(t.text_secondary))
        .text_size(px(t.type_caption))
        .mb(px(8.))
        .child(text.to_owned())
}

impl Shell {
    /// Runs `f` on the settings when the window has them.
    fn settings_mut(&mut self, f: impl FnOnce(&mut SettingsState)) {
        if let Some(settings) = self.settings.as_mut() {
            f(settings);
        }
    }

    /// The header strip with the Settings button, when the window has settings.
    pub(crate) fn header(&self, y: i32, width: i32, cx: &mut Context<Shell>) -> Option<Div> {
        self.settings.as_ref()?;
        let t = self.theme.clone();
        let button = chip(&t, "Settings", false, cx, |this| this.open_settings());
        Some(
            place(Rect::new(0, y, width, HEADER_HEIGHT))
                .bg(paint(t.popup))
                .flex()
                .justify_end()
                .items_center()
                .px(px(8.))
                .child(button),
        )
    }

    pub(crate) fn open_settings(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.open = true;
        }
    }

    pub(crate) fn close_settings(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.open = false;
        }
    }

    /// The overlay for a window of `area` size, or `None` while it is closed.
    pub(crate) fn settings_overlay(&mut self, area: Rect, cx: &mut Context<Shell>) -> Option<Div> {
        let t = self.theme.clone();
        let settings = self.settings.as_mut()?;
        if !settings.open {
            return None;
        }
        let w = CARD_WIDTH.min(area.w - 2 * CARD_MARGIN).max(0);
        let h = CARD_HEIGHT.min(area.h - 2 * CARD_MARGIN).max(0);
        let card_x = area.x + (area.w - w) / 2;
        let card_y = area.y + (area.h - h) / 2;

        // Tabs, then the page of the tab shown.
        let current = settings.panel.current();
        let mut tabs = div().flex().flex_wrap().mb(px(10.));
        for (index, title) in settings.panel.tabs().to_vec().into_iter().enumerate() {
            tabs = tabs.child(chip(&t, &title, index == current, cx, move |this| {
                if let Some(s) = this.settings.as_mut() {
                    s.panel.show(index);
                }
            }));
        }
        let body = match settings.panel.page_kind(current) {
            Some(PageKind::Audio) => match settings.audio_view() {
                Some(view) => audio_page(&t, &view, cx),
                None => note(&t, "No audio device model."),
            },
            Some(PageKind::Appearance) => appearance_page(&t, settings, cx),
            Some(PageKind::Midi) => midi_page(&t, &settings.prefs),
            Some(PageKind::Library) => library_page(&t, &settings.prefs),
            Some(PageKind::Plugins) => plugins_page(&t, &settings.prefs),
            None => note(&t, "This page is added by a plugin."),
        };
        let close = chip(&t, "Close", false, cx, |this| this.close_settings());

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
            .child(
                div()
                    .text_size(px(t.type_heading))
                    .mb(px(8.))
                    .child("Settings"),
            )
            .child(tabs)
            .child(div().flex_1().overflow_hidden().child(body))
            .child(div().flex().justify_end().child(close))
            // Clicks on the card do not reach the scrim below.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            });

        let scrim = place(area)
            .bg(paint(t.backdrop_top))
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

fn audio_page(t: &ThemeTokens, view: &AudioView, cx: &mut Context<Shell>) -> Div {
    let mut types = Vec::new();
    for name in &view.types {
        let (lit, name) = (*name == view.device_type, name.clone());
        types.push(chip(t, &name.clone(), lit, cx, move |this| {
            let name = name.clone();
            this.settings_mut(|s| s.edit_audio(|page| page.set_device_type(&name)));
        }));
    }
    let mut devices = Vec::new();
    for name in &view.devices {
        let (lit, name) = (*name == view.device, name.clone());
        devices.push(chip(t, &name.clone(), lit, cx, move |this| {
            let name = name.clone();
            this.settings_mut(|s| s.edit_audio(|page| page.set_output_device(&name)));
        }));
    }
    let mut rates = Vec::new();
    for rate in &view.rates {
        let rate = *rate;
        rates.push(chip(
            t,
            &format!("{rate} Hz"),
            rate == view.rate,
            cx,
            move |this| {
                this.settings_mut(|s| s.edit_audio(|page| page.set_sample_rate(rate)));
            },
        ));
    }
    let mut buffers = Vec::new();
    for size in &view.buffers {
        let size = *size;
        buffers.push(chip(
            t,
            &format!("{size} samples"),
            size == view.buffer,
            cx,
            move |this| {
                this.settings_mut(|s| s.edit_audio(|page| page.set_buffer_size(size)));
            },
        ));
    }
    let mut master = Vec::new();
    for (index, label) in view.pairs.iter().enumerate() {
        master.push(chip(t, label, index == view.master, cx, move |this| {
            this.settings_mut(|s| s.edit_audio(|page| page.set_master_pair(index)));
        }));
    }
    let mut cue = vec![chip(t, "None", view.cue.is_none(), cx, |this| {
        this.settings_mut(|s| s.edit_audio(|page| page.set_cue_pair(None)));
    })];
    for (index, label) in view.pairs.iter().enumerate() {
        cue.push(chip(t, label, view.cue == Some(index), cx, move |this| {
            this.settings_mut(|s| s.edit_audio(|page| page.set_cue_pair(Some(index))));
        }));
    }

    let mut column = div().flex().flex_col();
    column = column.child(choice_row(t, "Driver", row_of(types)));
    column = column.child(choice_row(t, "Output device", row_of(devices)));
    column = column.child(choice_row(t, "Sample rate", row_of(rates)));
    column = column.child(choice_row(t, "Buffer size", row_of(buffers)));
    column = column.child(choice_row(t, "Master output", row_of(master)));
    column = column.child(choice_row(t, "Headphone output", row_of(cue)));
    column = column.child(note(
        t,
        "Master and headphone mixes go to the chosen stereo pairs of the output device.",
    ));
    column.child(
        div()
            .text_color(paint(t.text_secondary))
            .text_size(px(t.type_caption))
            .child(view.readout.clone()),
    )
}

fn row_of(chips: Vec<Div>) -> Div {
    chips
        .into_iter()
        .fold(div().flex().flex_wrap(), |row, c| row.child(c))
}

fn appearance_page(t: &ThemeTokens, settings: &mut SettingsState, cx: &mut Context<Shell>) -> Div {
    let choices = AppearanceSettingsPage::new(&mut settings.prefs, &settings.themes).choices();
    let stored = settings.prefs.theme.clone();
    let mut chips = Vec::new();
    for (id, name) in choices {
        let lit = id == stored;
        chips.push(chip(t, &name, lit, cx, move |this| {
            let id = id.clone();
            this.settings_mut(|s| {
                s.pick_theme(&id);
            });
            // The live switch: the window takes the new theme on the next frame.
            if let Some(s) = this.settings.as_ref() {
                this.theme = s.themes.current().clone();
            }
        }));
    }
    div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Theme", row_of(chips)))
        .child(note(
            t,
            "The theme applies at once and is saved with the other preferences.",
        ))
}

fn midi_page(t: &ThemeTokens, prefs: &Preferences) -> Div {
    let inputs = if prefs.midi.enable_all {
        "all inputs".to_owned()
    } else {
        format!("{} input(s) selected", prefs.midi.enabled_inputs.len())
    };
    div()
        .flex()
        .flex_col()
        .child(note(t, "MIDI input is not ported yet (om-midi, Todo 10)."))
        .child(note(t, &format!("Stored: {inputs}.")))
        .child(note(t, &format!("Map file: {}", prefs.midi.map_file)))
}

fn library_page(t: &ThemeTokens, prefs: &Preferences) -> Div {
    div()
        .flex()
        .flex_col()
        .child(note(
            t,
            "The library is not built yet (om-library, Todo 12).",
        ))
        .child(note(
            t,
            &format!(
                "Stored: scan on start {}, every {} minutes.",
                if prefs.library.scan_on_start {
                    "on"
                } else {
                    "off"
                },
                prefs.library.scan_interval_minutes
            ),
        ))
}

fn plugins_page(t: &ThemeTokens, prefs: &Preferences) -> Div {
    div()
        .flex()
        .flex_col()
        .child(note(
            t,
            "Plugins are not ported yet (om-plugins, Todo 14). No plugin is loaded.",
        ))
        .child(note(
            t,
            &format!(
                "Plugin folder: {}",
                prefs.effective_plugin_folder().display()
            ),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::settings::SettingsResult;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Records calls; fails a rate of 22 050 Hz so the error path runs.
    #[derive(Default)]
    struct FakeAudio {
        rate: u32,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl AudioSettingsModel for FakeAudio {
        fn device_types(&self) -> Vec<String> {
            vec!["Fake".into()]
        }
        fn device_type(&self) -> String {
            "Fake".into()
        }
        fn set_device_type(&mut self, _: &str) -> SettingsResult<()> {
            Ok(())
        }
        fn output_devices(&self) -> Vec<String> {
            vec!["Out".into()]
        }
        fn output_device(&self) -> String {
            "Out".into()
        }
        fn set_output_device(&mut self, _: &str) -> SettingsResult<()> {
            Ok(())
        }
        fn sample_rates(&self) -> Vec<u32> {
            vec![44_100, 48_000]
        }
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn set_sample_rate(&mut self, rate: u32) -> SettingsResult<()> {
            if rate == 22_050 {
                return Err("unsupported device configuration: 22050 Hz".into());
            }
            self.rate = rate;
            self.log.borrow_mut().push(format!("rate {rate}"));
            Ok(())
        }
        fn buffer_sizes(&self) -> Vec<u32> {
            vec![256, 512]
        }
        fn buffer_size(&self) -> u32 {
            512
        }
        fn set_buffer_size(&mut self, _: u32) -> SettingsResult<()> {
            Ok(())
        }
        fn output_channels(&self) -> usize {
            2
        }
        fn state_string(&self) -> String {
            format!("rate={}", self.rate)
        }
    }

    fn state(
        audio: Option<Box<dyn AudioSettingsModel>>,
        changes: Rc<RefCell<usize>>,
    ) -> SettingsState {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        SettingsState::new(SettingsOptions {
            prefs: Preferences::default(),
            themes,
            audio,
            audio_stats: None,
            on_changed: Box::new(move |_| *changes.borrow_mut() += 1),
        })
    }

    #[test]
    fn an_audio_change_notifies_once_and_repeating_it_does_not() {
        let changes = Rc::new(RefCell::new(0));
        let log = Rc::new(RefCell::new(Vec::new()));
        let audio = FakeAudio {
            rate: 48_000,
            log: Rc::clone(&log),
        };
        let mut s = state(Some(Box::new(audio)), Rc::clone(&changes));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        assert_eq!(*changes.borrow(), 1);
        // The model is asked both times (as in C++); only the stored state decides the change.
        assert_eq!(
            *log.borrow(),
            vec!["rate 44100".to_owned(), "rate 44100".to_owned()]
        );
        assert_eq!(s.prefs.audio.device_state, "rate=44100");
    }

    #[test]
    fn a_failed_audio_change_is_kept_as_the_readout_text_and_notifies_nobody() {
        let changes = Rc::new(RefCell::new(0));
        let audio = FakeAudio {
            rate: 48_000,
            ..FakeAudio::default()
        };
        let mut s = state(Some(Box::new(audio)), Rc::clone(&changes));
        s.edit_audio(|page| page.set_sample_rate(22_050));
        assert_eq!(*changes.borrow(), 0);
        assert!(s.setup_error.contains("22050"));
        // With no device the readout shows that error instead of "No audio device running".
        assert_eq!(
            stats_text(&AudioStats::default(), &s.setup_error),
            s.setup_error
        );
    }

    #[test]
    fn no_audio_model_leaves_the_audio_tab_out_and_edits_nothing() {
        let changes = Rc::new(RefCell::new(0));
        let mut s = state(None, Rc::clone(&changes));
        assert!(!s.panel.tabs().iter().any(|t| t == "Audio"));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        assert_eq!(*changes.borrow(), 0);
    }

    #[test]
    fn picking_a_theme_selects_it_live_and_an_unknown_id_changes_nothing() {
        let changes = Rc::new(RefCell::new(0));
        let mut s = state(None, Rc::clone(&changes));
        let before = s.themes.active_id().map(str::to_owned);
        assert!(!s.pick_theme("no.such.theme"));
        assert_eq!(s.themes.active_id().map(str::to_owned), before);
        assert_eq!(*changes.borrow(), 0);

        assert!(s.pick_theme("openmixer.daylight"));
        assert_eq!(s.themes.active_id(), Some("openmixer.daylight"));
        assert_eq!(s.prefs.theme, "openmixer.daylight");
        assert_eq!(*changes.borrow(), 1);
    }

    #[test]
    fn the_accent_falls_back_to_the_surface_when_the_theme_has_none() {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        let mut tokens = themes.current().clone();
        tokens.accents.clear();
        assert_eq!(accent_of(&tokens), tokens.surface_raised);
    }
}
