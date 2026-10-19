//! One deck card: header, overview and zoomed waveforms, pitch fader, hotcue pads, loop controls
//! and transport. Each part is drawn from its om-ui model; clicks call the model, which writes
//! through the string-id control surface. Waveforms are synthetic until the analysis crate exists.

use crate::app::Shell;
use crate::paint::{paint, place};
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, Bounds, Context, Div, MouseButton,
    MouseDownEvent, Pixels, Window,
};
use om_ui::geometry::Rect;
use om_ui::theme::ThemeTokens;
use om_ui::widgets::deck::{deck_layout, DeckLayout};
use om_ui::widgets::hotcues::{HotcuePads, Modifiers};
use om_ui::widgets::loops::{LoopControls, SIZE_NAMES};
use om_ui::widgets::transport::{TransportButton, TransportPanel};
use om_ui::waveform::View;
use std::f64::consts::TAU;

/// Sample rate assumed for the synthetic waveform when a deck reports none.
const FALLBACK_RATE: f64 = 44_100.0;

/// Deterministic stand-in for analysed peaks: a 0..1 amplitude at `seconds` into the track.
pub fn synthetic_peak(seconds: f64) -> f32 {
    let a = 0.55 * (seconds * TAU * 0.5).sin().abs()
        + 0.30 * (seconds * TAU * 3.1).sin().abs()
        + 0.15 * (seconds * TAU * 11.0).sin().abs();
    (a as f32).min(1.0)
}

/// Card for deck `index`, drawn at `card` (window coordinates).
pub fn deck_card(shell: &Shell, index: usize, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let l = deck_layout(Rect::sized(card.w, card.h), t);
    let text = deck.header.text();

    let mut root = place(card)
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.accent_for(index as i32)))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary));

    // Header: title, artist, key and BPM, elapsed and remaining time.
    root = root.child(
        place(l.header)
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(t.type_title))
                    .child(text.title.clone()),
            )
            .child(
                div()
                    .text_size(px(t.type_small))
                    .text_color(paint(t.text_secondary))
                    .child(text.artist.clone()),
            )
            .child(
                div()
                    .text_size(px(t.type_body))
                    .child(format!("{}   {} BPM", text.key, text.bpm)),
            )
            .child(
                div()
                    .text_size(px(t.type_mono))
                    .child(format!("{}   {}", text.elapsed, text.remaining)),
            ),
    );

    root = root.child(overview(shell, index, card, &l, cx));
    root = root.child(pitch(shell, index, card, &l, cx));
    if l.zoom_visible {
        root = root.child(zoom(shell, index, card, &l, cx));
    }
    root = root.child(hotcues(shell, index, card, &l, cx));
    root = root.child(loops(shell, index, card, &l, cx));
    root = root.child(transport(shell, index, card, &l, cx));
    root
}

fn overview(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.overview;
    let width = f64::from(area.w);
    let playhead = deck.overview.playhead_x(width);
    let rate = file_rate(shell, index);
    let length = shell.engine.deck(index).length_frames();
    let loaded = length > 0;
    let origin_x = card.x + area.x;
    let bar_count = (area.w / 3).max(1);
    let (wave_colour, head_colour, head_w) = (paint(t.wave_mid), paint(t.highlight), t.playhead_width);

    let wave = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let mid = h / 2.0;
            if loaded {
                for i in 0..bar_count {
                    let frac = f64::from(i) / f64::from(bar_count);
                    let seconds = frac * length as f64 / rate;
                    let amp = synthetic_peak(seconds) * mid * 0.9;
                    let x = i as f32 * 3.0;
                    if x >= w {
                        break;
                    }
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.origin.x + px(x), bounds.origin.y + px(mid - amp.max(1.0))),
                            size(px(2.0), px((amp * 2.0).max(2.0))),
                        ),
                        wave_colour,
                    ));
                }
            }
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + px(playhead as f32), bounds.origin.y),
                    size(px(head_w), px(h)),
                ),
                head_colour,
            ));
        },
    )
    .size_full();

    place(area)
        .bg(paint(t.well))
        .rounded(px(t.control_radius))
        .child(wave)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                let x = f32::from(ev.position.x) - origin_x as f32;
                this.decks[index].overview.seek_at(f64::from(x), width);
                cx.notify();
            }),
        )
}

fn pitch(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.pitch;
    let range = deck.pitch.range();
    let value = deck.pitch.fader_value();
    // Top of the track is the fastest rate.
    let frac = ((1.0 + range - value) / (2.0 * range)).clamp(0.0, 1.0);
    let knob_y = frac as f32 * (area.h as f32 - 12.0);
    let origin_y = card.y + area.y;
    let height = f64::from(area.h);

    place(area)
        .child(
            div()
                .absolute()
                .left(px(0.))
                .top(px(0.))
                .text_size(px(t.type_small))
                .child(deck.pitch.readout()),
        )
        .child(
            place(Rect::new(area.w / 2 - 2, 16, 4, (area.h - 16).max(1)))
                .bg(paint(t.well))
                .rounded(px(2.)),
        )
        .child(
            place(Rect::new(0, 16 + knob_y as i32, area.w, 12))
                .bg(paint(t.surface_raised))
                .border_1()
                .border_color(paint(t.border))
                .rounded(px(t.control_radius)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                let y = f64::from(f32::from(ev.position.y)) - f64::from(origin_y);
                let frac = (y - 16.0).clamp(0.0, height - 16.0) / (height - 16.0).max(1.0);
                let range = this.decks[index].pitch.range();
                this.decks[index].pitch.user_set(1.0 + range - frac * 2.0 * range);
                cx.notify();
            }),
        )
}

fn zoom(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.zoom;
    let width = f64::from(area.w);
    let view: View = deck.zoom.view(width);
    let cues = deck.zoom.cue_xs(&view);
    let loop_span = deck.zoom.loop_span(&view);
    let rate = file_rate(shell, index);
    let length = shell.engine.deck(index).length_frames();
    let loaded = length > 0;
    let playhead = view.x_of_frame(shell.engine.deck(index).position_frames());
    let _ = card;
    let accent = paint(t.accent_for(index as i32));
    let wave_colour = paint(t.wave_high);
    let cue_colour = paint(t.master);
    let head_colour = paint(t.highlight);
    let (cue_w, head_w) = (t.cue_marker_width, t.playhead_width);
    let (loop_looping, loop_idle) = (t.loop_active_alpha, t.loop_idle_alpha);

    let strip = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let mid = h / 2.0;
            if let Some((x0, x1, looping)) = loop_span {
                let colour = if looping { loop_looping } else { loop_idle };
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.origin.x + px(x0 as f32), bounds.origin.y),
                        size(px((x1 - x0) as f32), px(h)),
                    ),
                    accent_with_alpha(accent, colour),
                ));
            }
            if loaded {
                let mut x = 0.0_f32;
                while x < w {
                    let frame = view.frame_at_x(f64::from(x));
                    if frame >= 0.0 && frame < length as f64 {
                        let amp = synthetic_peak(frame / rate) * mid * 0.9;
                        window.paint_quad(fill(
                            Bounds::new(
                                point(bounds.origin.x + px(x), bounds.origin.y + px(mid - amp.max(1.0))),
                                size(px(1.0), px((amp * 2.0).max(1.0))),
                            ),
                            wave_colour,
                        ));
                    }
                    x += 2.0;
                }
            }
            for x in cues.iter().flatten() {
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.origin.x + px(*x as f32), bounds.origin.y),
                        size(px(cue_w), px(h)),
                    ),
                    cue_colour,
                ));
            }
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + px(playhead as f32), bounds.origin.y),
                    size(px(head_w), px(h)),
                ),
                head_colour,
            ));
        },
    )
    .size_full();

    place(area)
        .bg(paint(t.well))
        .rounded(px(t.control_radius))
        .child(strip)
        .on_scroll_wheel(cx.listener(move |this, ev: &gpui::ScrollWheelEvent, _, cx| {
            let dy = f32::from(ev.delta.pixel_delta(px(16.)).y);
            this.decks[index].zoom.wheel(dy);
            cx.notify();
        }))
}

fn hotcues(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let pads = HotcuePads::layout(l.hotcues, t.inner_gap);
    let lit = deck.hotcues.lit();
    let _ = card;
    let mut row = place(l.hotcues);
    for (i, rect) in pads.iter().enumerate() {
        let pad = place(*rect)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(t.control_radius))
            .border_1()
            .border_color(paint(t.border))
            .bg(paint(if lit[i] { t.accent_for(index as i32) } else { t.surface_raised }))
            .text_color(paint(if lit[i] { t.on_accent } else { t.text_primary }))
            .text_size(px(t.type_small))
            .child(format!("{}", i + 1))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let mods = Modifiers {
                        shift: ev.modifiers.shift,
                        alt: ev.modifiers.alt,
                        ctrl: ev.modifiers.control,
                    };
                    this.decks[index].hotcues.click(i, mods);
                    cx.notify();
                }),
            );
        row = row.child(pad);
    }
    row
}

fn loops(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let lay = LoopControls::layout(l.loops, t.inner_gap);
    let size_lit = deck.loops.size_lit();
    let _ = card;
    let mut area = place(l.loops);
    for (i, rect) in lay.sizes.iter().enumerate() {
        area = area.child(action(
            t,
            *rect,
            SIZE_NAMES[i],
            size_lit[i],
            index,
            cx,
            move |this| {
                this.decks[index].loops.press_size(i);
            },
        ));
    }
    let reloop_lit = deck.loops.reloop_lit();
    area = area
        .child(action(t, lay.halve, "1/2x", false, index, cx, move |this| {
            this.decks[index].loops.halve();
        }))
        .child(action(t, lay.double, "2x", false, index, cx, move |this| {
            this.decks[index].loops.double();
        }))
        .child(action(t, lay.exit, "Exit", false, index, cx, move |this| {
            this.decks[index].loops.exit();
        }))
        .child(action(t, lay.reloop, "Reloop", reloop_lit, index, cx, move |this| {
            this.decks[index].loops.reloop();
        }));
    area
}

fn transport(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let rects = TransportPanel::layout(l.transport, t.inner_gap);
    let lit = deck.transport.toggle_states();
    let _ = card;
    let mut row = place(l.transport);
    for (i, button) in TransportButton::ALL.into_iter().enumerate() {
        let label = match button {
            TransportButton::Play => deck.transport.play_label(),
            other => other.label(),
        };
        row = row.child(action(t, rects[i], label, lit[i], index, cx, move |this| {
            this.decks[index].transport.press(button);
        }));
    }
    row
}

/// A labelled push button at `rect` (card-local) that runs `on_press` on the deck when clicked.
fn action(
    t: &ThemeTokens,
    rect: Rect,
    label: &str,
    lit: bool,
    index: usize,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let _ = index;
    place(rect)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(if lit { t.accent_for(index as i32) } else { t.surface_raised }))
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

/// A colour with its alpha scaled to `alpha` (GPUI keeps the RGB of the source colour).
fn accent_with_alpha(colour: gpui::Rgba, alpha: f32) -> gpui::Rgba {
    gpui::Rgba { a: alpha, ..colour }
}

fn file_rate(shell: &Shell, index: usize) -> f64 {
    let rate = shell.engine.deck(index).file_sample_rate();
    if rate > 0.0 {
        rate
    } else {
        FALLBACK_RATE
    }
}
