//! The automix bar, in the header strip: the AUTOMIX caption and a segmented Off | On switch. With
//! automix on and idle, the transition length and the next track (and Start); while a pick runs,
//! the state word, a progress bar with the beats left, Skip and Cancel. The view-model is om-ui's
//! `AutomixBar` (which decides what is offered); this file lays it out inside the rect the header
//! layout gives it and forwards clicks. The switch and the length are edits of the Automix settings
//! (saved and applied through the settings state); the buttons are presses on the `automix.start`,
//! `automix.skip` and `automix.cancel` ids.

use crate::app::Shell;
use crate::controls::{face_button, face_button_with, metrics, segmented_even, tip_overlay};
use crate::font::{mixed, numeral, Numeral, TypeExt as _};
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, Stateful};
use om_core::{AutomixSnapshot, ParameterTarget};
use om_ui::automix_bar::{
    AutomixBar, CANCEL_WIDTH, LENGTH_WIDTH, PROGRESS_WIDTH, REMAINING_WIDTH, ROW_GAP, ROW_PADDING,
    SKIP_WIDTH, START_WIDTH, STATE_WIDTH, SWITCH_WIDTH, TOGGLE_LABEL,
};
use om_ui::geometry::Rect;
use om_ui::preferences::AutomixPrefs;
use om_ui::settings::LENGTH_CHOICES;
use om_ui::style::Face;
use om_ui::theme::{ThemeTokens, TypeRole};

/// Reads the automix snapshot on the window thread (the service is shared, so this is cheap).
pub type AutomixSnapshotFn = std::sync::Arc<dyn Fn() -> AutomixSnapshot + Send + Sync>;

/// A chip in the bar: a face button `width` wide, so the row is as wide as the layout test adds up.
fn bar_chip(
    t: &ThemeTokens,
    (label, width): (&str, f32),
    cx: &mut Context<Shell>,
    press: impl Fn(&mut Shell) + 'static,
) -> Stateful<Div> {
    let rest = (Face::Rest, t.text_primary);
    face_button(t, label, rest, cx, move |this, _, _| press(this))
        .flex_none()
        .h_full()
        .w(px(width))
        .px(px(0.))
}

/// The segmented Off | On switch: the lit half is the state, in the text colour (no deck owns it).
fn segmented_switch(t: &ThemeTokens, enabled: bool, cx: &mut Context<Shell>) -> Div {
    let lit = (Some(usize::from(enabled)), t.text_primary);
    segmented_even(t, &["Off", "On"], lit, cx, |this: &mut Shell, i| {
        this.settings_mut(|s| s.automix_set_enabled(i == 1))
    })
    .w(px(SWITCH_WIDTH))
}

/// The transition length, one line in mono; a press steps to the next choice.
fn length_chip(t: &ThemeTokens, length: i32, next: i32, cx: &mut Context<Shell>) -> Stateful<Div> {
    let text = numeral(t, Numeral::Value, format!("{length} beats"));
    let rest = (Face::Rest, t.text_primary);
    face_button_with(t, "Transition length", text, rest, cx, move |this, _, _| {
        this.settings_mut(|s| s.automix_set_length(i64::from(next)))
    })
    .flex_none()
    .h_full()
    .w(px(LENGTH_WIDTH))
    .px(px(0.))
    .child(tip_overlay(t, "automix-length-tip", "Transition length"))
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
    let m = metrics(t);
    // The bar belongs to no deck: its progress is drawn in the text colour, not a deck hue.
    let accent = t.text_primary;
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
        .type_role(t, TypeRole::Label)
        .text_color(paint(t.text_primary))
        .child(
            div()
                .flex_none()
                .text_color(paint(t.text_muted))
                .child(TOGGLE_LABEL),
        )
        .child(segmented_switch(t, enabled, cx));
    if enabled && !bar.running {
        row = row.child(length_chip(t, length, next_length, cx));
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
            // An error is stated in words; red is kept for the playhead and recording.
            (bar.error.clone(), t.text_primary)
        };
        row = row.child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_color(paint(colour))
                .child(mixed(t, TypeRole::Label, &text)),
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
                        .h(px(m.space[2]))
                        .rounded(px(t.control_radius))
                        .bg(paint(t.surface_raised))
                        .border_1()
                        .border_color(paint(t.border))
                        .child(
                            div()
                                .h_full()
                                .w(px(PROGRESS_WIDTH * fraction))
                                .rounded(px(t.control_radius))
                                .bg(paint(accent)),
                        ),
                )
                .child(
                    numeral(t, Numeral::Value, bar.remaining.clone())
                        .w(px(REMAINING_WIDTH))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(paint(t.text_secondary)),
                ),
        );
    }
    if actions.start {
        row = row.child(bar_chip(t, ("Start", START_WIDTH), cx, |this| {
            this.press_automix("automix.start")
        }));
    }
    if actions.skip {
        row = row.child(bar_chip(t, ("Skip", SKIP_WIDTH), cx, |this| {
            this.press_automix("automix.skip")
        }));
    }
    if actions.cancel {
        row = row.child(bar_chip(t, ("Cancel", CANCEL_WIDTH), cx, |this| {
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
