//! One deck card: header, overview and zoomed waveforms, pitch fader, hotcue pads, loop controls
//! and transport. Each part is drawn from its om-ui model; clicks call the model, which writes
//! through the string-id control surface. The waveforms are the analysed peaks the library loader
//! holds for the deck (none until the track has been analysed). A library track dropped on the card
//! loads into this deck.

use crate::app::Shell;
use crate::controls::{button as push_button, knob, slider};
use crate::drag::Target;
use crate::library_view::TrackDragPayload;
use crate::paint::{paint, place, skin};
use gpui::{
    canvas, div, fill, point, prelude::*, px, relative, size, Bounds, Context, Div, MouseButton,
    MouseDownEvent, Pixels, Window,
};
use om_analysis::{WaveformData, WaveformPoint};
use om_ui::geometry::Rect;
use om_ui::stem_binding::{MAX_VOLUME, STEM_NAMES};
use om_ui::style::LookAndFeel;
use om_ui::waveform::{column, BandBalance, View};
use om_ui::widgets::deck::{deck_controls_layout, deck_layout, DeckLayout};
use om_ui::widgets::hotcues::{HotcuePads, Modifiers};
use om_ui::widgets::loops::{LoopControls, SIZE_NAMES};
use om_ui::widgets::stage::STAGE_DECKS;
use om_ui::widgets::stem_pads::stem_pad_layout;
use om_ui::widgets::transport::{TransportButton, TransportPanel};
use std::sync::Arc;

/// The loudest sample of a waveform point, 0..1 (full-band peak).
pub fn peak(p: &WaveformPoint) -> f32 {
    p.min.abs().max(p.max.abs()).min(1.0)
}

/// Card for deck `index`, drawn at `card` (window coordinates).
pub fn deck_card(shell: &Shell, index: usize, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let drop_target = t.drop_target;
    // A stage deck's header, overview and zoomed waveform are drawn by the stage above.
    let area = Rect::sized(card.w, card.h);
    let l = if index < STAGE_DECKS {
        deck_controls_layout(area, t)
    } else {
        deck_layout(area, t)
    };

    let mut root = place(card)
        .overflow_hidden()
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.accent_for(index as i32)))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
        // A library track dropped on the card loads into this deck.
        .drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(paint(drop_target)))
        .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
            if let Some(panel) = this.library.as_mut() {
                let _ = panel.drop_on_deck(index, &payload.description);
            }
            cx.notify();
        }));

    if !l.header.is_empty() {
        root = root.child(header(shell, index, &l));
    }
    if !l.overview.is_empty() {
        root = root.child(overview(shell, index, card, &l, cx));
    }
    root = root.child(pitch(shell, index, card, &l, cx));
    if l.zoom_visible {
        root = root.child(zoom(shell, index, card, &l, cx));
    }
    root = root.child(hotcues(shell, index, card, &l, cx));
    root = root.child(loops(shell, index, card, &l, cx));
    root = root.child(transport(shell, index, card, &l, cx));
    for part in stems(shell, index, card, &l, cx) {
        root = root.child(part);
    }
    root
}

/// The deck header: title and elapsed/remaining time on the first line, artist and key/BPM on the
/// second. Clipped to its rectangle so it never runs into the overview strip.
pub(crate) fn header(shell: &Shell, index: usize, l: &DeckLayout) -> Div {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let text = shell.decks[index].header.text();
    place(l.header)
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .overflow_hidden()
                        .text_size(px(t.type_title))
                        .child(text.title.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .pl(px(8.))
                        .flex()
                        .items_baseline()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(look.readout.value_size))
                                .line_height(relative(1.1))
                                .text_color(paint(look.readout.value))
                                .child(text.elapsed.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(look.readout.small_size))
                                .line_height(relative(1.1))
                                .text_color(paint(look.readout.unit))
                                .child(text.remaining.clone()),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .text_size(px(t.type_small))
                .text_color(paint(t.text_secondary))
                .child(div().overflow_hidden().child(text.artist.clone()))
                .child(
                    div()
                        .flex_none()
                        .pl(px(8.))
                        .flex()
                        .items_baseline()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(look.readout.small_size))
                                .line_height(relative(1.1))
                                .text_color(paint(look.readout.value))
                                .child(text.key.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(look.readout.small_size))
                                .line_height(relative(1.1))
                                .text_color(paint(look.readout.value))
                                .child(text.bpm.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(look.readout.unit_size))
                                .line_height(relative(1.1))
                                .text_color(paint(look.readout.unit))
                                .child("BPM"),
                        ),
                ),
        )
}

/// The stems row (S7): a load/off control with its caption, then one pad per stem. A pad is live
/// only while the deck plays from stems; otherwise the row is greyed and the caption says how to
/// get stems. Taps and drags write through the deck's `StemBinding`.
fn stems(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Vec<gpui::Div> {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let pads = &shell.decks[index].stems;
    if let Some(panel) = &shell.library {
        pads.observe_track(panel.loaded_track_id(index));
    }
    let states = pads.stems();
    let live = states[0].available;
    let accent = t.accent_for(index as i32);
    let layout = stem_pad_layout(l.stems, t);
    let abs = |r: Rect| Rect::new(card.x + r.x, card.y + r.y, r.w, r.h);
    let row_h = om_ui::widgets::px(t.deck_row_height);
    let mut parts = Vec::new();

    // Control column: load or switch the stems.
    let ctl = layout.control;
    let button_rect = Rect::new(ctl.x, ctl.y, ctl.w, row_h);
    let label = if live { "Stems off" } else { "Load stems" };
    parts.push(push_button(
        t,
        button_rect,
        label,
        live,
        accent,
        cx,
        move |shell: &mut Shell| shell.toggle_deck_stems(index),
    ));

    if !live {
        // Greyed: one box across the pads with the last message, or the prepare hint. No handlers.
        let first = layout.pads[0].mute;
        let last = layout.pads[layout.pads.len() - 1].solo;
        let span = Rect::new(first.x, first.y, last.x + last.w - first.x, l.stems.h);
        let message = shell.stem_errors[index]
            .clone()
            .or_else(|| {
                shell
                    .library
                    .as_ref()
                    .and_then(|panel| panel.stem_note(index))
            })
            .unwrap_or_else(|| "no stems \u{2014} Prepare stems".to_owned());
        parts.push(
            skin(place(span), &look.fader.track)
                .flex()
                .items_center()
                .justify_center()
                .text_color(paint(t.text_secondary))
                .text_size(px(t.type_small))
                .child(message),
        );
        return parts;
    }

    for (stem, state) in states.iter().enumerate() {
        let pad = layout.pads[stem];
        let name = STEM_NAMES[stem];
        let audible = !state.muted;
        let soloed = pads.soloed() == Some(stem);
        parts.push(push_button(
            t,
            pad.mute,
            name,
            audible,
            accent,
            cx,
            move |shell: &mut Shell| {
                shell.decks[index].stems.toggle_mute(stem);
            },
        ));
        // Activity glow: the pad's header fills in proportion to what the stem contributes.
        let level = pads.level(stem);
        if level > 0.0 {
            let w = (pad.mute.w as f32 * level) as i32;
            parts.push(
                place(Rect::new(pad.mute.x, pad.mute.y, w, pad.mute.h))
                    .bg(accent_with_alpha(paint(accent), 0.3))
                    .rounded(gpui::px(t.control_radius)),
            );
        }
        parts.push(push_button(
            t,
            pad.solo,
            "S",
            soloed,
            accent,
            cx,
            move |shell: &mut Shell| {
                shell.decks[index].stems.solo(stem);
            },
        ));
        parts.push(slider(
            t,
            pad.volume,
            (f64::from(state.gain), 0.0, MAX_VOLUME),
            Target::StemVolume(index, stem),
            abs(pad.volume),
            cx,
        ));
        let filter_readout = format!("{:+.2}", state.filter);
        parts.push(knob(
            t,
            pad.filter,
            ("Filter", filter_readout.as_str()),
            (f64::from(state.filter), -1.0, 1.0),
            Target::StemFilter(index, stem),
            abs(pad.filter),
            cx,
        ));
    }
    parts
}

pub(crate) fn overview(
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
    let bar_count = area.w.max(1);
    let (head_colour, head_w) = (paint(t.highlight), t.playhead_width);
    let tokens = t.clone();

    let canvas_el = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let mid = h / 2.0;
            if let Some(data) = &wave {
                let points = &data.overview().points;
                if !points.is_empty() {
                    let balance = BandBalance::of(points);
                    for i in 0..bar_count {
                        // One pixel column folds the points it covers, so the colour follows the
                        // spectrum under it.
                        let lo = i as usize * points.len() / bar_count as usize;
                        let hi = ((i as usize + 1) * points.len() / bar_count as usize)
                            .clamp(lo + 1, points.len());
                        let col = column(&points[lo..hi]);
                        let amp = col.peak * mid * 0.9;
                        let x = i as f32;
                        if x >= w {
                            break;
                        }
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    bounds.origin.x + px(x),
                                    bounds.origin.y + px(mid - amp.max(1.0)),
                                ),
                                size(px(1.0), px((amp * 2.0).max(2.0))),
                            ),
                            paint(balance.colour(&col, &tokens)),
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

pub(crate) fn zoom(
    shell: &Shell,
    index: usize,
    _card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.zoom;
    let width = f64::from(area.w);
    let view: View = deck.zoom.view(width);
    let cues = deck.zoom.cue_xs(&view);
    let grid = deck.zoom.grid_lines(&view);
    let grid_colour = paint(t.text_secondary);
    let (bar_alpha, beat_alpha) = (t.grid_bar_alpha, t.grid_beat_alpha);
    let loop_span = deck.zoom.loop_span(&view);
    let wave: Option<Arc<WaveformData>> = shell.library.as_ref().and_then(|p| p.waveform(index));
    let fpp = deck.zoom.frames_per_pixel();
    let length = shell.engine.deck(index).length_frames();
    let playhead = view.x_of_frame(shell.engine.deck(index).position_frames());
    let accent = paint(t.accent_for(index as i32));
    let tokens = t.clone();
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
                let balance = BandBalance::of(&data.overview().points);
                let mut x = 0.0_f32;
                while x < w {
                    let frame = view.frame_at_x(f64::from(x));
                    if frame >= 0.0 && frame < length as f64 {
                        // The points under this pixel column (one or a few, per the zoom level).
                        let lo = (frame / per_point) as usize;
                        let hi = (((frame + fpp) / per_point).ceil() as usize)
                            .max(lo + 1)
                            .min(level.points.len());
                        if let Some(slice) = level.points.get(lo..hi) {
                            let col = column(slice);
                            let amp = col.peak * mid * 0.9;
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        bounds.origin.x + px(x),
                                        bounds.origin.y + px(mid - amp.max(1.0)),
                                    ),
                                    size(px(1.0), px((amp * 2.0).max(1.0))),
                                ),
                                paint(balance.colour(&col, &tokens)),
                            ));
                        }
                    }
                    x += 1.0;
                }
            }
            for line in &grid {
                let (alpha, width) = if line.bar {
                    (bar_alpha, 2.0)
                } else {
                    (beat_alpha, 1.0)
                };
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.origin.x + px(line.x as f32), bounds.origin.y),
                        size(px(width), px(h)),
                    ),
                    accent_with_alpha(grid_colour, alpha),
                ));
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
    let mut row = place(Rect::sized(card.w, card.h));
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
    // One frame: the lit buttons and the reloop state come from the same deck snapshot.
    let frame = deck.loops.frame();
    let mut area = place(Rect::sized(card.w, card.h));
    for (i, rect) in lay.sizes.iter().enumerate() {
        area = area.child(push_button(
            t,
            *rect,
            SIZE_NAMES[i],
            frame.size_lit[i],
            t.accent_for(index as i32),
            cx,
            move |this| {
                this.decks[index].loops.press_size(i);
            },
        ));
    }
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
            frame.reloop_lit,
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
    let mut row = place(Rect::sized(card.w, card.h));
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
