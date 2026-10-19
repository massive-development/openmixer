//! One deck card: header, overview and zoomed waveforms, pitch fader, hotcue pads, loop controls
//! and transport. Each part is drawn from its om-ui model; clicks call the model, which writes
//! through the string-id control surface. The waveforms are the analysed peaks the library loader
//! holds for the deck (none until the track has been analysed). A library track dropped on the card
//! loads into this deck.

use crate::app::Shell;
use crate::controls::button as push_button;
use crate::drag::Target;
use crate::library_view::TrackDragPayload;
use crate::paint::{paint, place};
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, Bounds, Context, Div, MouseButton,
    MouseDownEvent, Pixels, Window,
};
use om_analysis::{WaveformData, WaveformPoint};
use om_ui::geometry::Rect;
use om_ui::waveform::View;
use om_ui::widgets::deck::{deck_layout, DeckLayout};
use om_ui::widgets::hotcues::{HotcuePads, Modifiers};
use om_ui::widgets::loops::{LoopControls, SIZE_NAMES};
use om_ui::widgets::transport::{TransportButton, TransportPanel};
use std::sync::Arc;

/// The loudest sample of a waveform point, 0..1 (full-band peak).
pub fn peak(p: &WaveformPoint) -> f32 {
    p.min.abs().max(p.max.abs()).min(1.0)
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
        .text_color(paint(t.text_primary))
        // A library track dropped on the card loads into this deck.
        .drag_over::<TrackDragPayload>(|style, _, _, _| style.bg(gpui::rgba(0x2a_6f_9e_40)))
        .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
            if let Some(panel) = this.library.as_mut() {
                let _ = panel.drop_on_deck(index, &payload.description);
            }
            cx.notify();
        }));

    // Header: title, artist, key and BPM, elapsed and remaining time.
    root = root.child(
        place(l.header)
            .flex()
            .flex_col()
            .child(div().text_size(px(t.type_title)).child(text.title.clone()))
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

fn overview(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.overview;
    let width = f64::from(area.w);
    let playhead = deck.overview.playhead_x(width);
    let wave: Option<Arc<WaveformData>> = shell.library.as_ref().and_then(|p| p.waveform(index));
    let origin_x = card.x + area.x;
    let bar_count = (area.w / 3).max(1);
    let (wave_colour, head_colour, head_w) =
        (paint(t.wave_mid), paint(t.highlight), t.playhead_width);

    let canvas_el = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let mid = h / 2.0;
            if let Some(data) = &wave {
                let points = &data.overview().points;
                if !points.is_empty() {
                    for i in 0..bar_count {
                        let frac = f64::from(i) / f64::from(bar_count);
                        let idx = ((frac * points.len() as f64) as usize).min(points.len() - 1);
                        let amp = peak(&points[idx]) * mid * 0.9;
                        let x = i as f32 * 3.0;
                        if x >= w {
                            break;
                        }
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    bounds.origin.x + px(x),
                                    bounds.origin.y + px(mid - amp.max(1.0)),
                                ),
                                size(px(2.0), px((amp * 2.0).max(2.0))),
                            ),
                            wave_colour,
                        ));
                    }
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
        .child(canvas_el)
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
    let abs_pitch = Rect::new(card.x + area.x, card.y + area.y, area.w, area.h);

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
                this.begin_drag(Target::Pitch(index), abs_pitch, ev.position);
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
    let wave: Option<Arc<WaveformData>> = shell.library.as_ref().and_then(|p| p.waveform(index));
    let fpp = deck.zoom.frames_per_pixel();
    let length = shell.engine.deck(index).length_frames();
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
            if let Some(data) = &wave {
                let level = data.level_for(fpp);
                let per_point = level.samples_per_point.max(1) as f64;
                let mut x = 0.0_f32;
                while x < w {
                    let frame = view.frame_at_x(f64::from(x));
                    if frame >= 0.0 && frame < length as f64 {
                        if let Some(p) = level.points.get((frame / per_point) as usize) {
                            let amp = peak(p) * mid * 0.9;
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        bounds.origin.x + px(x),
                                        bounds.origin.y + px(mid - amp.max(1.0)),
                                    ),
                                    size(px(1.0), px((amp * 2.0).max(1.0))),
                                ),
                                wave_colour,
                            ));
                        }
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
        .on_scroll_wheel(
            cx.listener(move |this, ev: &gpui::ScrollWheelEvent, _, cx| {
                let dy = f32::from(ev.delta.pixel_delta(px(16.)).y);
                this.decks[index].zoom.wheel(dy);
                cx.notify();
            }),
        )
}

fn hotcues(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Div {
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
            .bg(paint(if lit[i] {
                t.accent_for(index as i32)
            } else {
                t.surface_raised
            }))
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
        area = area.child(push_button(
            t,
            *rect,
            SIZE_NAMES[i],
            size_lit[i],
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].loops.press_size(i);
            },
        ));
    }
    let reloop_lit = deck.loops.reloop_lit();
    area = area
        .child(push_button(
            t,
            lay.halve,
            "1/2x",
            false,
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].loops.halve();
            },
        ))
        .child(push_button(
            t,
            lay.double,
            "2x",
            false,
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].loops.double();
            },
        ))
        .child(push_button(
            t,
            lay.exit,
            "Exit",
            false,
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].loops.exit();
            },
        ))
        .child(push_button(
            t,
            lay.reloop,
            "Reloop",
            reloop_lit,
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].loops.reloop();
            },
        ));
    area
}

fn transport(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Div {
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
        row = row.child(push_button(
            t,
            rects[i],
            label,
            lit[i],
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].transport.press(button);
            },
        ));
    }
    row
}

/// A colour with its alpha replaced by `alpha` (GPUI keeps the RGB of the source colour).
fn accent_with_alpha(colour: gpui::Rgba, alpha: f32) -> gpui::Rgba {
    gpui::Rgba { a: alpha, ..colour }
}
