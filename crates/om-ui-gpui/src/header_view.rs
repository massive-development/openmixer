//! The header strip (comp 4a/4b): the OPENMIXER wordmark in the display family, the automix bar,
//! then the latency and dropout indicator, the loudness toggle, Record, the library toggle and
//! Settings, under one etched line. The rects come from om-ui's `header_layout` (through
//! `main_layout::page_for`); every part shares one top and one height. This file only draws them.
//! The master tempo is drawn over the mixer column (`deck_view::tempo_card`).

use crate::app::Shell;
use crate::controls::button;
use crate::font::{numeral, Numeral, TypeExt as _};
use crate::loudness_paint::{loudness_panel, loudness_toggle};
use crate::paint::{paint, place};
use gpui::{prelude::*, Context, Div};
use om_ui::geometry::Rect;
use om_ui::header_layout::{HeaderNeeds, HeaderRects};
use om_ui::labels::latency_text;
use om_ui::theme::TypeRole;
use om_ui::widgets::loudness::panel_rect;

/// The wordmark (the display family is for the wordmark and headings at 21 px and up).
pub const WORDMARK: &str = "OPENMIXER";

impl Shell {
    /// Which optional header parts this window has.
    pub(crate) fn header_needs(&self) -> HeaderNeeds {
        HeaderNeeds {
            automix: self.options.automix.is_some(),
            record: self.options.record,
            settings: self.settings.is_some(),
            // The banner is a toast now (see `Shell::show_banner`), not a header chip.
            notice: false,
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
            .child(place(Rect::new(0, rects.bar.h - 1, rects.bar.w, 1)).bg(paint(t.border)));
        strip = strip.child(
            place(rects.brand)
                .flex()
                .items_center()
                .whitespace_nowrap()
                .type_role(&t, TypeRole::Headline)
                .text_color(paint(t.text_primary))
                .child(WORDMARK),
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
                .justify_end()
                .whitespace_nowrap()
                .text_color(paint(if healthy { t.text_muted } else { t.warning }))
                .child(numeral(&t, Numeral::Value, text)),
        );
        strip = strip.children(loudness_toggle(self, rects.loudness, cx));
        if let Some(rect) = rects.record {
            let snapshot = self.engine.recorder().snapshot();
            strip = strip.child(crate::record_button::record_button(&t, &snapshot, rect, cx));
        }
        strip = strip.child(button(
            &t,
            rects.library_toggle,
            "LIBRARY",
            !self.performance,
            t.text_primary,
            cx,
            |this| this.performance = !this.performance,
        ));
        if let Some(rect) = rects.settings {
            strip = strip.child(button(
                &t,
                rect,
                "Settings",
                false,
                t.text_primary,
                cx,
                |this| this.open_settings(),
            ));
        }
        strip
    }

    /// The open loudness panel, dropped down under the header's loudness toggle.
    pub(crate) fn loudness_overlay(
        &self,
        rects: &HeaderRects,
        window_w: i32,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !self.loudness.is_open() {
            return None;
        }
        let gap = crate::controls::metrics(&self.theme).space[1].round() as i32;
        let rect = panel_rect(rects.loudness, rects.bar.bottom(), gap, window_w);
        Some(loudness_panel(self, rect, cx))
    }
}
