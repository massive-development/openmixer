//! `--debug-panel`: device, latency and CPU figures, the router's unknown-id count and a
//! raw list of the control surface's current values. The row builder is pure and tested headless;
//! the window only draws the rows.

use crate::app::Shell;
use crate::paint::{paint, skin};
use crate::text_entry::{apply_key, EntryKey};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent};
use om_core::param_id::DeckParam;
use om_engine::DjEngine;
use om_ui::binding::DeckBinding;
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;
use std::path::PathBuf;
use std::sync::Arc;

/// The per-deck load and play controls: each deck's file path, the field being typed in, and the
/// line that reports the last load or play.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeckControls {
    pub paths: Vec<String>,
    /// The deck whose path field takes typed keys.
    pub editing: Option<usize>,
    /// The last outcome: its text, and whether it is an error.
    pub status: Option<(String, bool)>,
}

impl DeckControls {
    pub fn new(decks: usize) -> Self {
        Self {
            paths: vec![String::new(); decks],
            editing: None,
            status: None,
        }
    }

    /// Opens the path field of `deck` for typing. Out-of-range decks are ignored.
    pub fn begin(&mut self, deck: usize) {
        if deck < self.paths.len() {
            self.editing = Some(deck);
        }
    }

    /// Routes a key to the open field, with the settings overlay's rules (see [`apply_key`]).
    /// `None` when no field is open or the key does nothing to it. Enter and Escape close the field.
    pub fn key(&mut self, key: &str, typed: Option<&str>) -> Option<EntryKey> {
        let deck = self.editing?;
        let outcome = apply_key(&mut self.paths[deck], key, typed);
        if matches!(outcome, EntryKey::Submit | EntryKey::Cancel) {
            self.editing = None;
        }
        (outcome != EntryKey::Ignored).then_some(outcome)
    }

    /// The typed path for `deck`, trimmed. An empty path is a message, not a path.
    pub fn path_for(&self, deck: usize) -> Result<PathBuf, String> {
        match self.paths.get(deck).map(|p| p.trim()) {
            None | Some("") => Err("type a file path first".to_owned()),
            Some(path) => Ok(PathBuf::from(path)),
        }
    }

    pub fn report(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some((text.into(), error));
    }
}

/// The value that toggles a deck's play state: 0 pauses a playing deck, 1 starts a stopped one.
pub fn play_value(playing: bool) -> f64 {
    if playing {
        0.0
    } else {
        1.0
    }
}

impl Shell {
    /// Loads the typed path of `deck` through the library's loader. A playing deck refuses, and
    /// the refusal is shown in the status line.
    pub(crate) fn debug_load(&mut self, deck: usize) {
        let path = match self.deck_controls.path_for(deck) {
            Ok(path) => path,
            Err(message) => return self.deck_controls.report(message, true),
        };
        let Some(panel) = self.library.as_mut() else {
            return self.deck_controls.report(
                "the library is not available, so files cannot be loaded",
                true,
            );
        };
        match panel.load_path(deck, &path, false) {
            Ok(()) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.deck_controls
                    .report(format!("loaded {name} into deck {}", deck + 1), false);
            }
            Err(error) => self.deck_controls.report(error.to_string(), true),
        }
    }

    /// Play or pause `deck` through its binding, as the transport does.
    pub(crate) fn debug_toggle_play(&mut self, deck: usize) {
        let playing = self.engine.deck(deck).is_playing();
        let binding = DeckBinding::typed(Arc::clone(&self.engine), Arc::clone(&self.router), deck);
        if binding.send(DeckParam::Play, play_value(playing)) {
            let verb = if playing { "paused" } else { "playing" };
            self.deck_controls
                .report(format!("deck {} {verb}", deck + 1), false);
        } else {
            self.deck_controls
                .report("the engine did not take the play command", true);
        }
    }

    /// A key while the debug column has focus: an open path field takes it; Enter loads.
    pub(crate) fn debug_key(&mut self, key: &str, typed: Option<&str>) {
        let deck = self.deck_controls.editing;
        if self.deck_controls.key(key, typed) == Some(EntryKey::Submit) {
            if let Some(deck) = deck {
                self.debug_load(deck);
            }
        }
    }
}

/// The Decks section: per deck a label, Play/Pause and Load buttons, and the path field under them.
/// `playing` holds each deck's play state; the status line sits under the last deck.
pub fn decks_section(
    t: &ThemeTokens,
    controls: &DeckControls,
    playing: &[bool],
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let gap = px(t.inner_gap as f32);
    let mut section = div().flex().flex_col().gap(gap);
    for (deck, path) in controls.paths.iter().enumerate() {
        let is_playing = playing.get(deck).copied().unwrap_or(false);
        let buttons = div()
            .flex()
            .items_center()
            .gap(gap)
            .child(div().flex_1().child(format!("Deck {}", deck + 1)))
            .child(chip(
                t,
                if is_playing { "Pause" } else { "Play" },
                cx,
                move |this| this.debug_toggle_play(deck),
            ))
            .child(chip(t, "Load", cx, move |this| this.debug_load(deck)));
        let editing = controls.editing == Some(deck);
        let shown = if editing {
            format!("{path}|")
        } else if path.is_empty() {
            "click to type a file path".to_owned()
        } else {
            path.clone()
        };
        let field_style = if editing {
            look.text_field.focus
        } else {
            look.text_field.idle
        };
        let field = skin(
            div()
                .w_full()
                .overflow_hidden()
                .px(px(look.inset_large * 2.0))
                .py(px(look.inset_small * 2.0)),
            &field_style,
        )
        .child(shown)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                this.deck_controls.begin(deck);
                if let Some(focus) = this.debug_focus.clone() {
                    focus.focus(window);
                }
                cx.notify();
            }),
        );
        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(look.inset_small))
                .child(buttons)
                .child(field),
        );
    }
    if let Some((text, error)) = &controls.status {
        let line = if *error {
            format!("Error: {text}")
        } else {
            text.clone()
        };
        section = section.child(
            div()
                .text_size(px(t.type_caption))
                .text_color(paint(if *error {
                    t.text_primary
                } else {
                    t.text_secondary
                }))
                .child(line),
        );
    }
    section
}

/// A small push button for the debug column; `on_press` runs on the shell.
fn chip(
    t: &ThemeTokens,
    label: &str,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let hover = look.button.hover.fill;
    skin(
        div()
            .px(px(look.inset_large * 2.0))
            .py(px(look.inset_small * 2.0))
            .flex()
            .items_center()
            .justify_center(),
        &look.button.idle,
    )
    .hover(move |style| style.bg(paint(hover)))
    .child(label.to_owned())
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            on_press(this);
            cx.notify();
        }),
    )
}

/// What the window shows about the audio device. The binary fills it in from its device.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceStatus {
    pub name: String,
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub round_trip_ms: f64,
    pub callback_avg_us: f64,
    pub callback_max_us: f64,
    pub budget_us: f64,
    pub cpu_load: f64,
    pub xruns: u64,
    pub callbacks: u64,
}

/// Lines of the device section, `(label, value)`.
pub fn device_rows(status: &DeviceStatus) -> Vec<(String, String)> {
    vec![
        ("device".to_owned(), status.name.clone()),
        (
            "sample rate".to_owned(),
            format!("{:.0} Hz", status.sample_rate),
        ),
        (
            "buffer".to_owned(),
            format!("{} frames", status.buffer_size),
        ),
        (
            "round trip".to_owned(),
            format!("{:.2} ms", status.round_trip_ms),
        ),
        (
            "callback avg / max".to_owned(),
            format!(
                "{:.0} / {:.0} us (budget {:.0} us)",
                status.callback_avg_us, status.callback_max_us, status.budget_us
            ),
        ),
        (
            "cpu load".to_owned(),
            format!("{:.1} %", status.cpu_load * 100.0),
        ),
        ("xruns".to_owned(), status.xruns.to_string()),
        ("callbacks".to_owned(), status.callbacks.to_string()),
    ]
}

/// Every control-surface value the models read, as `(label, id, value)`: the label in words (the
/// id itself where it has none), the id in router form for the developer.
pub fn parameter_rows(engine: &DjEngine, unknown: usize) -> Vec<(String, String, String)> {
    parameter_values(engine, unknown)
        .into_iter()
        .map(|(id, value)| (om_ui::labels::param_label(&id), id, value))
        .collect()
}

fn parameter_values(engine: &DjEngine, unknown: usize) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mixer = engine.mixer();
    rows.push(("router.unknown".to_owned(), unknown.to_string()));
    for deck in 0..engine.num_decks() {
        let d = engine.deck(deck);
        let p = format!("deck.{deck}");
        rows.push((format!("{p}.rate"), format!("{:.4}", d.rate())));
        rows.push((format!("{p}.playing"), d.is_playing().to_string()));
        rows.push((format!("{p}.position"), d.position_frames().to_string()));
        rows.push((format!("{p}.length"), d.length_frames().to_string()));
        rows.push((format!("{p}.looping"), d.is_looping().to_string()));
        rows.push((format!("{p}.keylock"), d.is_keylock().to_string()));
        rows.push((
            format!("sync.enabled.{deck}"),
            engine.sync().is_enabled(deck).to_string(),
        ));
    }
    for ch in 0..mixer.num_channels() {
        let c = format!("mixer.ch{ch}");
        rows.push((format!("{c}.trim"), format!("{:.3}", mixer.trim(ch))));
        rows.push((format!("{c}.fader"), format!("{:.3}", mixer.fader(ch))));
        rows.push((format!("{c}.filter"), format!("{:.3}", mixer.filter(ch))));
        for band in 0..3 {
            rows.push((
                format!("{c}.eq.{band}"),
                format!("{:.2}", mixer.eq(ch, band)),
            ));
        }
        rows.push((format!("{c}.pfl"), mixer.pfl(ch).to_string()));
        rows.push((
            format!("{c}.xfader"),
            format!("{:?}", mixer.crossfader_assign(ch)),
        ));
    }
    rows.push((
        "mixer.master".to_owned(),
        format!("{:.3}", mixer.master_gain()),
    ));
    rows.push((
        "mixer.crossfader".to_owned(),
        format!("{:.3}", mixer.crossfader()),
    ));
    rows.push((
        "mixer.crossfader.curve".to_owned(),
        format!("{:?}", mixer.crossfader_curve()),
    ));
    rows.push((
        "mixer.cue.mix".to_owned(),
        format!("{:.3}", mixer.cue_mix()),
    ));
    rows.push((
        "mixer.cue.level".to_owned(),
        format!("{:.3}", mixer.cue_level()),
    ));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_field_takes_typed_text_and_enter_submits_it() {
        let mut c = DeckControls::new(2);
        assert_eq!(c.key("a", Some("a")), None, "no field open");
        c.begin(1);
        assert_eq!(c.key("/", Some("/")), Some(EntryKey::Edited));
        assert_eq!(c.key("x", Some("x")), Some(EntryKey::Edited));
        assert_eq!(c.paths[1], "/x");
        assert_eq!(c.paths[0], "", "other decks are untouched");
        assert_eq!(c.key("enter", None), Some(EntryKey::Submit));
        assert_eq!(c.editing, None, "enter closes the field");
    }

    #[test]
    fn an_empty_path_is_a_message_and_escape_cancels() {
        let mut c = DeckControls::new(1);
        c.begin(0);
        assert_eq!(c.key("escape", None), Some(EntryKey::Cancel));
        assert_eq!(c.editing, None);
        assert_eq!(c.path_for(0), Err("type a file path first".to_owned()));
        c.paths[0] = "  /music/a.wav ".into();
        assert_eq!(c.path_for(0), Ok(PathBuf::from("/music/a.wav")));
        assert!(c.path_for(3).is_err(), "no such deck");
    }

    #[test]
    fn play_toggles_between_zero_and_one() {
        assert_eq!(play_value(false), 1.0);
        assert_eq!(play_value(true), 0.0);
    }

    #[test]
    fn parameter_rows_use_the_router_ids_and_read_the_engine() {
        let (engine, _audio) = DjEngine::new(2);
        engine.mixer().set_master_gain(0.25);
        let rows = parameter_rows(&engine, 3);
        let get = |id: &str| {
            rows.iter()
                .find(|(_, k, _)| k == id)
                .map(|(_, _, v)| v.clone())
        };
        let label = |id: &str| {
            rows.iter()
                .find(|(_, k, _)| k == id)
                .map(|(l, _, _)| l.clone())
        };
        assert_eq!(label("deck.1.rate").as_deref(), Some("Deck 2 Tempo"));
        assert_eq!(
            label("router.unknown").as_deref(),
            Some("Unrouted messages")
        );
        assert_eq!(get("router.unknown").as_deref(), Some("3"));
        assert_eq!(get("mixer.master").as_deref(), Some("0.250"));
        assert_eq!(get("deck.1.rate").as_deref(), Some("1.0000"));
        assert!(get("mixer.ch0.fader").is_some());
    }

    #[test]
    fn device_rows_show_the_latency_figures() {
        let rows = device_rows(&DeviceStatus {
            name: "mock".into(),
            sample_rate: 48_000.0,
            buffer_size: 256,
            round_trip_ms: 5.33,
            callback_avg_us: 120.0,
            callback_max_us: 300.0,
            budget_us: 5333.0,
            cpu_load: 0.025,
            xruns: 2,
            callbacks: 1000,
        });
        assert!(rows.iter().any(|(k, v)| k == "cpu load" && v == "2.5 %"));
        assert!(rows.iter().any(|(k, v)| k == "xruns" && v == "2"));
    }
}
