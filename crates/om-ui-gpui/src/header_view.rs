//! The header strip: the OPENMIXER wordmark, the automix bar, the notice chip, the master BPM,
//! the latency and dropout indicator, Record, the library toggle and Settings. The rects come from
//! om-ui's `header_layout` (through `main_layout::page_for`); every part shares one top and one
//! height, so the controls sit on one baseline. This file only draws them.

use crate::app::Shell;
use crate::controls::{button, tip_overlay};
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent};
use om_ui::header_layout::{HeaderNeeds, HeaderRects};
use om_ui::labels::latency_text;
use om_ui::widgets::deck::format_bpm;

/// What the collapsed notice chip says (the message itself is in its tooltip).
const NOTICE_SHORT: &str = "Notice";

impl Shell {
    /// Which optional header parts this window has.
    pub(crate) fn header_needs(&self) -> HeaderNeeds {
        HeaderNeeds {
            automix: self.options.automix.is_some(),
            record: self.options.record,
            settings: self.settings.is_some(),
            notice: self.options.banner.is_some() && !self.notice_dismissed,
        }
    }

    /// The tempo the mix runs at: the sync leader's, else the first playing deck's, else deck 1's.
    /// Zero when no deck has a tempo.
    pub(crate) fn master_bpm(&self) -> f64 {
        let leader = self.engine.sync().leader();
        let playing = (0..self.decks.len()).find(|&d| self.engine.deck(d).is_playing());
        [leader, playing, Some(0)]
            .into_iter()
            .flatten()
            .filter_map(|d| self.decks.get(d))
            .map(|deck| deck.header.effective_bpm())
            .find(|bpm| *bpm > 0.0)
            .unwrap_or(0.0)
    }

    /// The whole header strip at the rects `rects` came out as.
    pub(crate) fn header_strip(&mut self, rects: &HeaderRects, cx: &mut Context<Self>) -> Div {
        let t = self.theme.clone();
        let mut strip = place(rects.bar)
            .bg(paint(t.popup))
            .border_b_1()
            .border_color(paint(t.border));

        strip = strip.child(
            place(rects.brand)
                .flex()
                .items_center()
                .pl(px(6.))
                .text_size(px(t.type_body))
                .text_color(paint(t.brand))
                .child("OPENMIXER"),
        );

        if let (Some(rect), Some(read)) = (rects.automix, self.options.automix.clone()) {
            let snapshot = read();
            let prefs = self
                .settings
                .as_ref()
                .map(|s| s.prefs.automix.clone())
                .unwrap_or_default();
            strip = strip.child(crate::automix_bar::automix_bar_view(
                &t, &snapshot, &prefs, rect, cx,
            ));
        }

        if let (Some(rect), Some(text)) = (rects.notice, self.options.banner.clone()) {
            let collapsed = rect.w < om_ui::header_layout::NOTICE_WIDTH;
            strip = strip.child(
                place(rect)
                    .flex()
                    .items_center()
                    .gap(px(if collapsed { 2. } else { 6. }))
                    .px(px(if collapsed { 6. } else { 8. }))
                    .whitespace_nowrap()
                    .rounded(px(t.control_radius))
                    .border_1()
                    .border_color(paint(t.border))
                    .bg(paint(t.surface_raised))
                    .text_size(px(t.type_caption))
                    .text_color(paint(t.text_primary))
                    .overflow_hidden()
                    .child(div().flex_1().min_w(px(0.)).child(if collapsed {
                        NOTICE_SHORT.to_owned()
                    } else {
                        text.clone()
                    }))
                    // The whole message on hover, so the collapsed chip is never cryptic.
                    .child(tip_overlay(&t, "notice-chip", text))
                    .child(
                        div()
                            .flex_none()
                            .px(px(4.))
                            .text_size(px(t.type_body))
                            .text_color(paint(t.text_secondary))
                            .child("\u{00D7}")
                            .child(tip_overlay(&t, "notice-dismiss", "Dismiss"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                                    this.notice_dismissed = true;
                                    cx.notify();
                                }),
                            ),
                    ),
            );
        }

        if let Some(rect) = rects.master_bpm {
            let bpm = self.master_bpm();
            strip = strip.child(
                place(rect)
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(t.type_readout_small))
                            .text_color(paint(t.brand))
                            .child(format_bpm(bpm)),
                    )
                    .child(
                        div()
                            .text_size(px(t.type_caption))
                            .text_color(paint(t.text_muted))
                            .child("BPM"),
                    ),
            );
        }

        let (text, healthy) = match self.settings.as_ref().and_then(|s| s.audio_snapshot()) {
            Some(stats) if stats.sample_rate > 0.0 => (
                latency_text(stats.output_latency_ms, stats.xrun_count),
                stats.xrun_count == 0,
            ),
            _ => ("No audio".to_owned(), true),
        };
        strip = strip.child(
            place(rects.latency)
                .flex()
                .items_center()
                .justify_center()
                .whitespace_nowrap()
                .text_size(px(t.type_small))
                .text_color(paint(if healthy { t.text_secondary } else { t.warning }))
                .child(text),
        );

        if let Some(rect) = rects.record {
            let snapshot = self.engine.recorder().snapshot();
            strip = strip.child(crate::record_button::record_button(&t, &snapshot, rect, cx));
        }

        strip = strip.child(button(
            &t,
            rects.library_toggle,
            "LIBRARY",
            !self.performance,
            t.accent_for(0),
            cx,
            |this| this.performance = !this.performance,
        ));
        if let Some(rect) = rects.settings {
            strip = strip.child(button(
                &t,
                rect,
                "Settings",
                false,
                t.accent_for(0),
                cx,
                |this| this.open_settings(),
            ));
        }
        strip
    }
}
