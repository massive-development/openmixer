//! The automix bar: the left part of the top bar. It is one AUTO toggle while automix is off; with
//! it on, the state, the next track, a progress bar with the beats left while a transition runs,
//! and only the buttons that do something right now (Start, Skip, Cancel), plus the last error.
//! The view-model is om-ui's `AutomixBar` (which decides what is offered); this file lays it out
//! and forwards clicks. The toggle is an edit of the Automix settings (saved and applied through
//! the settings state), as are style and length, which are set on the Settings page. The buttons
//! are presses on the `automix.start`, `automix.skip` and `automix.cancel` ids.

use crate::app::Shell;
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent};
use om_core::{AutomixSnapshot, ParameterTarget};
use om_ui::automix_bar::{
    AutomixBar, BAR_HEIGHT, PROGRESS_WIDTH, REMAINING_WIDTH, ROW_GAP, ROW_PADDING, STATE_WIDTH,
    TOGGLE_LABEL,
};
use om_ui::geometry::Rect;
use om_ui::preferences::AutomixPrefs;
use om_ui::theme::ThemeTokens;

/// Reads the automix snapshot on the window thread (the service is shared, so this is cheap).
pub type AutomixSnapshotFn = std::sync::Arc<dyn Fn() -> AutomixSnapshot + Send + Sync>;

/// The bar's strip for a window `width` at `y`.
pub fn bar_rect(y: i32, width: i32) -> Rect {
    Rect::new(0, y, width, BAR_HEIGHT)
}

/// Height of a chip in the bar, in px: the strip is 32.
const CHIP_HEIGHT: f32 = 26.0;

/// A chip in the bar: 12 px text, lit in the first accent. Unlike the settings chips it has no
/// margins, so the row's own gap spaces it.
fn bar_chip(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let accent = t.accents.first().copied().unwrap_or(t.surface_raised);
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(CHIP_HEIGHT))
        .px(px(10.))
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(if lit { accent } else { t.surface_raised }))
        .text_color(paint(if lit { t.on_accent } else { t.text_primary }))
        .text_size(px(t.type_caption.max(12.0)))
        .child(label.to_owned())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                on_press(this);
                cx.notify();
            }),
        )
}

/// Draws the bar from the snapshot and the stored preferences. It is compact: the AUTO toggle
/// alone while automix is off; with it on, the state, the next track and only the buttons that do
/// something right now. Style and length live in Settings.
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
    let actions = bar.actions();
    let fraction = bar.progress as f32;

    let mut row = div()
        .flex()
        .size_full()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(ROW_PADDING))
        .text_size(px(t.type_caption.max(12.0)))
        .text_color(paint(t.text_primary));
    row = row.child(bar_chip(t, TOGGLE_LABEL, enabled, cx, move |this| {
        this.settings_mut(|s| s.automix_set_enabled(!enabled))
    }));
    if bar.has_detail() && enabled {
        row = row.child(
            div()
                .flex_none()
                .w(px(STATE_WIDTH))
                .overflow_hidden()
                .whitespace_nowrap()
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
                                .bg(paint(t.accents.first().copied().unwrap_or(t.surface_hover))),
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
        row = row.child(bar_chip(t, "Start", false, cx, |this| {
            this.press_automix("automix.start")
        }));
    }
    if actions.skip {
        row = row.child(bar_chip(t, "Skip", false, cx, |this| {
            this.press_automix("automix.skip")
        }));
    }
    if actions.cancel {
        row = row.child(bar_chip(t, "Cancel", false, cx, |this| {
            this.press_automix("automix.cancel")
        }));
    }
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

impl Shell {
    /// Presses one of the `automix.*` presses through the router (start, skip or cancel).
    pub(crate) fn press_automix(&mut self, id: &str) {
        let _ = self.router.set_parameter(id, 1.0);
    }
}
