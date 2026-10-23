//! The automix bar: the left part of the header strip, with the Enable, style and length chips, the next
//! track, a progress bar with the beats left, Start now, Skip and Cancel, and the last error. The
//! view-model is om-ui's `AutomixBar`; this file lays it out and forwards clicks. Enable, style
//! and length are edits of the Automix settings (saved and applied through the settings state);
//! Start now, Skip and Cancel are presses on the `automix.start`, `automix.skip` and
//! `automix.cancel` ids; Skip only sends while the view-model says it does something.

use crate::app::Shell;
use crate::paint::{paint, place};
use crate::settings_view::chips::chip;
use gpui::{div, prelude::*, px, Context, Div};
use om_core::{AutomixSnapshot, ParameterTarget};
use om_ui::automix_bar::{
    AutomixBar, BAR_HEIGHT, PROGRESS_WIDTH, REMAINING_WIDTH, ROW_GAP, ROW_PADDING, STATE_WIDTH,
};
use om_ui::geometry::Rect;
use om_ui::preferences::AutomixPrefs;
use om_ui::settings::{LENGTH_CHOICES, STYLE_LABELS};
use om_ui::theme::ThemeTokens;

/// Reads the automix snapshot on the window thread (the service is shared, so this is cheap).
pub type AutomixSnapshotFn = std::sync::Arc<dyn Fn() -> AutomixSnapshot + Send + Sync>;

/// The bar's strip for a window `width` at `y`.
pub fn bar_rect(y: i32, width: i32) -> Rect {
    Rect::new(0, y, width, BAR_HEIGHT)
}

/// Draws the bar from the snapshot and the stored preferences.
pub(crate) fn automix_bar_view(
    t: &ThemeTokens,
    snapshot: &AutomixSnapshot,
    prefs: &AutomixPrefs,
    width: i32,
    y: i32,
    cx: &mut Context<Shell>,
) -> Div {
    let bar = AutomixBar::new(snapshot, prefs);
    let enabled = bar.enabled;
    let style_code = prefs.style;
    let next_style = (style_code + 1) % STYLE_LABELS.len() as i32;
    let length = bar.length_beats;
    let next_length = LENGTH_CHOICES
        .iter()
        .copied()
        .find(|&beats| beats > length)
        .unwrap_or(LENGTH_CHOICES[0]);
    let fraction = bar.progress as f32;

    let mut row = div()
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(ROW_PADDING))
        .text_size(px(t.type_small))
        .text_color(paint(t.text_primary));
    row = row.child(label(t, "Automix"));
    row = row.child(chip(
        t,
        if enabled { "On" } else { "Off" },
        enabled,
        cx,
        move |this| this.settings_mut(|s| s.automix_set_enabled(!enabled)),
    ));
    row = row.child(chip(t, bar.style, false, cx, move |this| {
        this.settings_mut(|s| s.automix_set_style(next_style))
    }));
    row = row.child(chip(
        t,
        &om_ui::settings::length_label(length),
        false,
        cx,
        move |this| this.settings_mut(|s| s.automix_set_length(i64::from(next_length))),
    ));
    row = row.child(
        div()
            .w(px(STATE_WIDTH))
            .overflow_hidden()
            .text_color(paint(t.text_secondary))
            .child(bar.state.to_owned()),
    );
    row = row.child(
        div()
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(bar.next.clone()),
    );
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
                            .bg(paint(t.accents.first().copied().unwrap_or(t.surface_hover))),
                    ),
            )
            .child(
                div()
                    .w(px(REMAINING_WIDTH))
                    .overflow_hidden()
                    .text_color(paint(t.text_secondary))
                    .child(bar.remaining.clone()),
            ),
    );
    row = row.child(chip(t, "Start now", bar.can_start, cx, |this| {
        this.press_automix("automix.start")
    }));
    let can_skip = bar.can_skip;
    row = row.child(chip(t, "Skip", can_skip, cx, move |this| {
        if can_skip {
            this.press_automix("automix.skip")
        }
    }));
    row = row.child(chip(t, "Cancel", bar.can_cancel, cx, |this| {
        this.press_automix("automix.cancel")
    }));
    if !bar.error.is_empty() {
        row = row.child(
            div()
                .max_w(px(240.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(paint(t.meter_high))
                .child(bar.error.clone()),
        );
    }
    place(bar_rect(y, width))
        .bg(paint(t.popup))
        .border_b_1()
        .border_color(paint(t.border))
        .child(row)
}

fn label(t: &ThemeTokens, text: &str) -> Div {
    div().text_color(paint(t.text_muted)).child(text.to_owned())
}

impl Shell {
    /// Presses one of the `automix.*` presses through the router (start, skip or cancel).
    pub(crate) fn press_automix(&mut self, id: &str) {
        let _ = self.router.set_parameter(id, 1.0);
    }
}
