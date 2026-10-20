//! The header's Record button and the `r` shortcut. The view-model is om-ui's `RecordBar`; this
//! file draws it with the theme's colours and sends `record.toggle` (the same id the button and
//! the key both use). While a text field or the settings card takes typed keys, `r` is a letter.

use crate::app::Shell;
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent};
use om_core::{ParameterTarget, RecordSnapshot};
use om_ui::geometry::Rect;
use om_ui::record_bar::RecordBar;
use om_ui::theme::ThemeTokens;

/// Width of each channel's peak track in the button.
const PEAK_WIDTH: f32 = 36.0;
/// Height of each channel's peak track.
const PEAK_HEIGHT: f32 = 3.0;

/// The Record button at `rect`: an outline while idle; while recording, a filled red dot, the
/// elapsed time, the size, a dropped-frames badge (when any) and the mini L/R peak bars.
pub(crate) fn record_button(
    t: &ThemeTokens,
    snapshot: &RecordSnapshot,
    rect: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let bar = RecordBar::new(snapshot, "");
    let red = paint(t.record_red);
    let mut dot = div()
        .flex_none()
        .w(px(10.))
        .h(px(10.))
        .rounded(px(5.))
        .border_1()
        .border_color(red);
    if bar.recording {
        dot = dot.bg(red);
    }
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .size_full()
        .overflow_hidden()
        .text_size(px(t.type_small))
        .text_color(paint(t.text_primary))
        .child(dot)
        .child(
            div()
                .flex_none()
                .text_color(paint(if bar.failed {
                    t.meter_high
                } else {
                    t.text_primary
                }))
                .child(bar.label.clone()),
        );
    if bar.recording {
        row = row.child(
            div()
                .flex_none()
                .text_color(paint(t.text_secondary))
                .child(bar.size.clone()),
        );
    }
    if !bar.warning.is_empty() {
        row = row.child(
            div()
                .flex_none()
                .px(px(4.))
                .rounded(px(3.))
                .bg(paint(t.meter_mid))
                .text_color(paint(t.on_accent))
                .child(format!("!{}", snapshot.dropped_frames)),
        );
    }
    if bar.recording {
        row = row.child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(peak_track(t, bar.peak_left))
                .child(peak_track(t, bar.peak_right)),
        );
    }
    let mut button = place(rect)
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(if bar.recording { red } else { paint(t.border) })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.press_record();
                cx.notify();
            }),
        );
    if bar.recording {
        button = button.bg(paint(t.surface_raised));
    }
    button.child(row)
}

/// One channel's peak: a thin track with the level filled in.
fn peak_track(t: &ThemeTokens, level: f32) -> Div {
    div()
        .w(px(PEAK_WIDTH))
        .h(px(PEAK_HEIGHT))
        .rounded(px(1.5))
        .bg(paint(t.well))
        .child(
            div()
                .h_full()
                .w(px(PEAK_WIDTH * level))
                .rounded(px(1.5))
                .bg(paint(t.meter_low)),
        )
}

impl Shell {
    /// Sends `record.toggle`, as the Record button and the `r` key do.
    pub(crate) fn press_record(&mut self) {
        let _ = self.router.set_parameter("record.toggle", 1.0);
    }

    /// True while something takes typed keys (the settings card, a text field in the library,
    /// the sampler's path field or the debug column), so `r` must not toggle recording.
    pub(crate) fn typing(&self) -> bool {
        self.settings.as_ref().is_some_and(|s| s.open)
            || self.sampler_editing
            || self.deck_controls.editing.is_some()
            || self.library.as_ref().is_some_and(|panel| panel.is_typing())
    }
}
