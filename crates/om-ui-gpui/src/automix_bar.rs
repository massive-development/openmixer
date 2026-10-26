//! The automix bar, in the header strip: the AUTOMIX caption and a segmented Off | On switch. With
//! automix on and idle, the transition length and the next track (and Start); while a pick runs,
//! the state word, a progress bar with the beats left, Skip and Cancel. The view-model is om-ui's
//! `AutomixBar` (which decides what is offered); this file lays it out inside the rect the header
//! layout gives it and forwards clicks. The switch and the length are edits of the Automix settings
//! (saved and applied through the settings state); the buttons are presses on the `automix.start`,
//! `automix.skip` and `automix.cancel` ids.

use crate::app::Shell;
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent};
use om_core::{AutomixSnapshot, ParameterTarget};
use om_ui::automix_bar::{
    AutomixBar, LENGTH_WIDTH, PROGRESS_WIDTH, REMAINING_WIDTH, ROW_GAP, ROW_PADDING, STATE_WIDTH,
    SWITCH_WIDTH, TOGGLE_LABEL,
};
use om_ui::geometry::Rect;
use om_ui::preferences::AutomixPrefs;
use om_ui::settings::LENGTH_CHOICES;
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;

/// Reads the automix snapshot on the window thread (the service is shared, so this is cheap).
pub type AutomixSnapshotFn = std::sync::Arc<dyn Fn() -> AutomixSnapshot + Send + Sync>;

/// A press handler for the bar's chips.
fn on_press(el: Div, cx: &mut Context<Shell>, press: impl Fn(&mut Shell) + 'static) -> Div {
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            press(this);
            cx.notify();
        }),
    )
}

/// A chip in the bar, as tall as the bar's content. The row's own gap spaces it.
fn bar_chip(
    t: &ThemeTokens,
    label: &str,
    height: f32,
    cx: &mut Context<Shell>,
    press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let chip = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(height))
        .px(px(10.))
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(t.surface_raised))
        .text_color(paint(t.text_primary))
        .text_size(px(t.type_small))
        .whitespace_nowrap()
        .child(label.to_owned());
    on_press(chip, cx, press)
}

/// The segmented Off | On switch: the lit half is the state. Both halves, the divider and the
/// frame come from the theme's toggle style.
fn segmented_switch(t: &ThemeTokens, enabled: bool, height: f32, cx: &mut Context<Shell>) -> Div {
    let toggle = LookAndFeel::from_tokens(t).toggle;
    let half = |label: &str, lit: bool, target: bool, cx: &mut Context<Shell>| {
        let style = if lit { toggle.on } else { toggle.off };
        let el = div()
            .flex()
            .flex_1()
            .h_full()
            .items_center()
            .justify_center()
            .bg(paint(style.fill))
            .text_color(paint(style.text))
            .text_size(px(style.font_size))
            .child(label.to_owned());
        on_press(el, cx, move |this| {
            this.settings_mut(|s| s.automix_set_enabled(target))
        })
    };
    div()
        .flex()
        .flex_none()
        .w(px(SWITCH_WIDTH))
        .h(px(height))
        .rounded(px(toggle.on.radius))
        .border_1()
        .border_color(paint(toggle.on.border))
        .overflow_hidden()
        .child(half("Off", !enabled, false, cx))
        .child(
            div()
                .flex_none()
                .w(px(toggle.knob.border_width.round().max(1.0)))
                .h_full()
                .bg(paint(toggle.knob.fill)),
        )
        .child(half("On", enabled, true, cx))
}

/// Draws the bar into `rect` from the snapshot and the stored preferences. Style and the default
/// length live in Settings; the length chip steps through the usual choices.
pub(crate) fn automix_bar_view(
    t: &ThemeTokens,
    snapshot: &AutomixSnapshot,
    prefs: &AutomixPrefs,
    rect: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let bar = AutomixBar::new(snapshot, prefs);
    let enabled = bar.enabled;
    let actions = bar.actions();
    let fraction = bar.progress as f32;
    let height = rect.h as f32;
    let accent = t.accents.first().copied().unwrap_or(t.surface_hover);
    let length = bar.length_beats;
    let next_length = LENGTH_CHOICES
        .iter()
        .copied()
        .find(|&beats| beats > length)
        .unwrap_or(LENGTH_CHOICES[0]);

    let mut row = div()
        .flex()
        .size_full()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(ROW_PADDING))
        .text_size(px(t.type_small))
        .text_color(paint(t.text_primary))
        .child(
            div()
                .flex_none()
                .text_size(px(t.type_caption))
                .text_color(paint(t.text_muted))
                .child(TOGGLE_LABEL),
        )
        .child(segmented_switch(t, enabled, height, cx));
    if enabled && !bar.running {
        let chip = div()
            .flex()
            .flex_none()
            .flex_col()
            .items_center()
            .justify_center()
            .w(px(LENGTH_WIDTH))
            .h(px(height))
            .rounded(px(t.control_radius))
            .border_1()
            .border_color(paint(t.border))
            .bg(paint(t.surface_raised))
            .text_size(px(t.type_small))
            .text_color(paint(t.text_primary))
            .child(
                div()
                    .text_size(px(t.type_caption))
                    .text_color(paint(t.text_muted))
                    .child("Transition"),
            )
            .child(format!("{length} beats"));
        row = row.child(on_press(chip, cx, move |this| {
            this.settings_mut(|s| s.automix_set_length(i64::from(next_length)))
        }));
    }
    if enabled && bar.running {
        row = row.child(
            div()
                .flex_none()
                .w(px(STATE_WIDTH))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(paint(t.text_secondary))
                .child(bar.state.to_owned()),
        );
    }
    if enabled || !bar.error.is_empty() {
        let (text, colour) = if bar.error.is_empty() {
            (bar.next.clone(), t.text_primary)
        } else {
            (bar.error.clone(), t.meter_high)
        };
        row = row.child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(paint(colour))
                .child(text),
        );
    } else {
        row = row.child(div().flex_1());
    }
    if bar.shows_progress() {
        row = row.child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(ROW_GAP))
                .child(
                    div()
                        .w(px(PROGRESS_WIDTH))
                        .h(px(8.))
                        .rounded(px(4.))
                        .bg(paint(t.surface_raised))
                        .border_1()
                        .border_color(paint(t.border))
                        .child(
                            div()
                                .h_full()
                                .w(px(PROGRESS_WIDTH * fraction))
                                .rounded(px(4.))
                                .bg(paint(accent)),
                        ),
                )
                .child(
                    div()
                        .w(px(REMAINING_WIDTH))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(paint(t.text_secondary))
                        .child(bar.remaining.clone()),
                ),
        );
    }
    if actions.start {
        row = row.child(bar_chip(t, "Start", height, cx, |this| {
            this.press_automix("automix.start")
        }));
    }
    if actions.skip {
        row = row.child(bar_chip(t, "Skip", height, cx, |this| {
            this.press_automix("automix.skip")
        }));
    }
    if actions.cancel {
        row = row.child(bar_chip(t, "Cancel", height, cx, |this| {
            this.press_automix("automix.cancel")
        }));
    }
    place(rect).child(row)
}

impl Shell {
    /// Presses one of the `automix.*` presses through the router (start, skip or cancel).
    pub(crate) fn press_automix(&mut self, id: &str) {
        let _ = self.router.set_parameter(id, 1.0);
    }
}

#[cfg(test)]
mod tests {
    /// User-visible strings never show a router id: the settings pages and the automix bar write
    /// words (`om_ui::labels`), and ids appear only in tests and the developer panel.
    #[test]
    fn no_dotted_ids_in_user_strings() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![dir.join("automix_bar.rs")];
        for entry in std::fs::read_dir(dir.join("settings_view")).expect("settings_view dir") {
            files.push(entry.expect("entry").path());
        }
        for file in files {
            let text = std::fs::read_to_string(&file).expect("source");
            let production = text.split("#[cfg(test)]").next().unwrap_or_default();
            for (n, line) in production.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") || !code.contains('"') {
                    continue;
                }
                for id in ["deck.0", "mixer.ch"] {
                    assert!(
                        !code.contains(id),
                        "{}:{}: a literal shows `{id}`: {code}",
                        file.display(),
                        n + 1
                    );
                }
            }
        }
    }
}
