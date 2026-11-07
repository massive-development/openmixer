//! `--debug-panel`: device, latency and CPU figures, the router's unknown-id count and a
//! raw list of the control surface's current values. The row builder is pure and tested headless;
//! the window only draws the rows. The per-deck load and play controls are `gpui-component`
//! buttons and text inputs.

use crate::app::Shell;
use crate::controls::push_button;
use crate::paint::paint;
use gpui::{div, prelude::*, px, Context, Div, Entity, Window};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use om_core::param_id::DeckParam;
use om_engine::DjEngine;
use om_ui::binding::DeckBinding;
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;
use std::path::PathBuf;
use std::sync::Arc;

/// Height of a debug-column button or path field, px.
const CONTROL_H: f32 = 22.0;

/// The per-deck load and play controls: each deck's file path field and the line that reports the
/// last load or play. The fields are made on the first draw (they need a window).
#[derive(Default)]
pub struct DeckControls {
    paths: Vec<Option<Entity<InputState>>>,
    /// The last outcome: its text, and whether it is an error.
    pub status: Option<(String, bool)>,
}

impl DeckControls {
    pub fn new(decks: usize) -> Self {
        Self {
            paths: vec![None; decks],
            status: None,
        }
    }

    /// The path fields that exist so far (the shell asks them whether one has the keyboard).
    pub fn inputs(&self) -> impl Iterator<Item = &Entity<InputState>> {
        self.paths.iter().flatten()
    }

    /// The typed path for `deck`, trimmed. An empty path is a message, not a path.
    pub fn path_for(&self, deck: usize, cx: &gpui::App) -> Result<PathBuf, String> {
        let typed = self
            .paths
            .get(deck)
            .and_then(Option::as_ref)
            .map(|input| input.read(cx).value().trim().to_owned());
        match typed.as_deref() {
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
    /// Makes the path field of every deck that has none yet. Enter in a field loads its deck.
    pub(crate) fn ensure_deck_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for deck in 0..self.deck_controls.paths.len() {
            if self.deck_controls.paths[deck].is_some() {
                continue;
            }
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder("file path (Enter loads)"));
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.debug_load(deck, cx);
                    cx.notify();
                }
            })
            .detach();
            self.deck_controls.paths[deck] = Some(input);
        }
    }

    /// Loads the typed path of `deck` through the library's loader. A playing deck refuses, and
    /// the refusal is shown in the status line.
    pub(crate) fn debug_load(&mut self, deck: usize, cx: &gpui::App) {
        let path = match self.deck_controls.path_for(deck, cx) {
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
    for (deck, input) in controls.paths.iter().enumerate() {
        let is_playing = playing.get(deck).copied().unwrap_or(false);
        let action = |label: &'static str, cx: &mut Context<Shell>| {
            div().w(px(54.)).h(px(CONTROL_H)).child(
                push_button(t, label, None, cx, move |this, _, cx| match label {
                    "Load" => this.debug_load(deck, cx),
                    _ => this.debug_toggle_play(deck),
                })
                .w_full()
                .h_full(),
            )
        };
        let buttons = div()
            .flex()
            .items_center()
            .gap(gap)
            .child(div().flex_1().child(format!("Deck {}", deck + 1)))
            .child(action(if is_playing { "Pause" } else { "Play" }, cx))
            .child(action("Load", cx));
        let mut block = div()
            .flex()
            .flex_col()
            .gap(px(look.inset_small))
            .child(buttons);
        if let Some(input) = input {
            block = block.child(div().w_full().child(Input::new(input)));
        }
        section = section.child(block);
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
                    look.panel.text
                } else {
                    look.panel_caption.text
                }))
                .child(line),
        );
    }
    section
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
