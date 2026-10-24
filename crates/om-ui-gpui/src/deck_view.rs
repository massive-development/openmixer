//! One deck card: header, overview and zoomed waveforms, pitch fader, transport, hotcue pads, loop
//! row and a tab strip that swaps the pad area for the stems or the More toggles. Each part is drawn from its om-ui model; clicks call the model, which writes
//! through the string-id control surface. The waveforms are the analysed peaks the library loader
//! holds for the deck (none until the track has been analysed). A library track dropped on the card
//! loads into this deck.

use crate::app::Shell;
use crate::controls::slider;
use crate::drag::Target;
use crate::library_view::TrackDragPayload;
use crate::paint::{paint, place, shadow, skin};
use gpui::{
    canvas, div, fill, point, prelude::*, px, relative, size, Bounds, Context, Div, MouseButton,
    MouseDownEvent, Pixels, Window,
};
use om_analysis::{WaveformData, WaveformPoint};
use om_ui::geometry::Rect;
use om_ui::stem_binding::{MAX_VOLUME, STEM_NAMES};
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;
use om_ui::waveform::{overview_column, strip_column, wave_colour, View};
use om_ui::widgets::deck::{
    deck_controls_layout_for, deck_layout_for, tab_rects, DeckLayout, PadMode, CONTROL_GAP,
    STEM_ROW_GAP,
};
use om_ui::widgets::deck_header::{
    header_metrics, ARTIST_SIZE, BADGE_SIZE, ELAPSED_SIZE, MIN_TEXT_SIZE, ON_BRIGHT, SYNC_ON,
    TITLE_SIZE, WARN_RED,
};
use om_ui::widgets::hotcues::{HotcuePads, Modifiers};
use om_ui::widgets::loops::LoopControls;
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
    let mode = shell.decks[index].pad_mode;
    let l = if index < STAGE_DECKS {
        deck_controls_layout_for(area, t, mode)
    } else {
        deck_layout_for(area, t, mode)
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
    root = root.child(transport(shell, index, card, &l, cx));
    root = root.child(tabs(shell, index, &l, card, cx));
    match mode {
        PadMode::Cues => root = root.child(hotcues(shell, index, card, &l, cx)),
        PadMode::More => root = root.child(more(shell, index, &l, card, cx)),
        PadMode::Stems => {
            for part in stems(shell, index, card, &l, cx) {
                root = root.child(part);
            }
        }
    }
    if !l.loops.is_empty() {
        root = root.child(loops(shell, index, card, &l, cx));
    }
    root
}

/// How a [`tile`] is drawn.
#[derive(Clone, Copy)]
enum Tile {
    Idle,
    /// Lit: this fill with this text colour, and a glow.
    Lit(om_ui::theme::Colour, om_ui::theme::Colour),
}

/// A pressable box: `label` at `size` (never below the 12 px floor), lit or idle, calling
/// `on_press` on a left press. Every tile the deck draws is at least 32 px in both directions.
fn tile(
    t: &ThemeTokens,
    rect: Rect,
    label: &str,
    (size, state): (f32, Tile),
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let (fill, text) = match state {
        Tile::Idle => (look.button.idle.fill, look.button.idle.text),
        Tile::Lit(fill, text) => (fill, text),
    };
    let el = place(rect)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(look.button.idle.border))
        .bg(paint(fill))
        .text_color(paint(text))
        .text_size(px(size.max(MIN_TEXT_SIZE)));
    let el = match state {
        Tile::Lit(fill, _) => shadow(el, &look.glow(fill)),
        Tile::Idle => {
            let hover = look.button.hover.fill;
            el.hover(move |style| style.bg(paint(hover)))
        }
    };
    el.child(label.to_owned()).on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            on_press(this);
            cx.notify();
        }),
    )
}

/// A small state chip in the header (SYNC, MASTER, LOOP 1/4), shown only while it is on.
fn chip(label: &str, fill: om_ui::theme::Colour, text: om_ui::theme::Colour) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(paint(fill))
        .text_color(paint(text))
        .text_size(px(MIN_TEXT_SIZE))
        .font_weight(gpui::FontWeight::BOLD)
        .child(label.to_owned())
}

/// The deck header. Left to right: the deck letter badge, title and artist over the state chips
/// (SYNC, MASTER, LOOP size, each shown only while on), then the large numerals: effective BPM,
/// key (one notation), and the time remaining, which is the biggest number and turns red under 30
/// seconds, with the elapsed time under it. Titles never wrap; they end in an ellipsis. Clipped to
/// its rectangle so it never runs into the overview strip.
pub(crate) fn header(shell: &Shell, index: usize, l: &DeckLayout) -> Div {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let text = shell.decks[index].header.text();
    let metrics = header_metrics(l.header.w);
    let accent = t.accent_for(index as i32);
    let remaining_colour = if text.remaining_warn {
        WARN_RED
    } else {
        look.readout.value
    };

    let mut chips = div().h(px(18.)).flex().items_center().gap(px(6.));
    if text.sync {
        chips = chips.child(chip("SYNC", SYNC_ON, ON_BRIGHT));
    }
    if text.master {
        chips = chips.child(chip("MASTER", t.master, t.on_accent));
    }
    if let Some(len) = &text.loop_label {
        chips = chips.child(chip(&format!("LOOP {len}"), accent, t.on_accent));
    }

    let numeral = |value: String, size: f32, colour| {
        div()
            .text_size(px(size))
            .line_height(relative(1.0))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(paint(colour))
            .child(value)
    };
    let caption = |label: &str| {
        div()
            .text_size(px(MIN_TEXT_SIZE))
            .line_height(relative(1.0))
            .text_color(paint(look.readout.unit))
            .child(label.to_owned())
    };

    place(l.header)
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .w(px(BADGE_SIZE as f32))
                .h(px(BADGE_SIZE as f32))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(t.control_radius))
                .bg(paint(accent))
                .text_color(paint(t.on_accent))
                .text_size(px(TITLE_SIZE))
                .font_weight(gpui::FontWeight::BOLD)
                .child(text.letter.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(2.))
                .child(div().truncate().text_size(px(TITLE_SIZE)).child(text.title))
                .child(
                    div()
                        .truncate()
                        .text_size(px(ARTIST_SIZE))
                        .text_color(paint(t.text_secondary))
                        .child(text.artist),
                )
                .child(chips),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.))
                .child(numeral(text.bpm, metrics.number, look.readout.value))
                .child(caption("BPM")),
        )
        .child(
            div()
                .flex_none()
                .child(numeral(text.key, metrics.number, look.readout.value)),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(2.))
                .child(numeral(text.remaining, metrics.time, remaining_colour))
                .child(
                    div()
                        .text_size(px(ELAPSED_SIZE))
                        .line_height(relative(1.0))
                        .text_color(paint(look.readout.unit))
                        .child(text.elapsed),
                ),
        )
}

/// Whether the deck plays from stems; also tells the stem model which track is loaded.
fn stems_live(shell: &Shell, index: usize) -> bool {
    let pads = &shell.decks[index].stems;
    if let Some(panel) = &shell.library {
        pads.observe_track(panel.loaded_track_id(index));
    }
    pads.stems()[0].available
}

/// The stems drawer: one row per stem with a mute button carrying its name, solo, a volume slider
/// and a filter slider with a centre detent (no numbers except while the filter is dragged). A deck
/// not playing from stems shows how to get them instead.
fn stems(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Vec<gpui::Div> {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let mut parts = Vec::new();
    if l.stems.is_empty() {
        return parts;
    }
    let pads = &shell.decks[index].stems;
    let live = stems_live(shell, index);
    let accent = t.accent_for(index as i32);
    let abs = |r: Rect| Rect::new(card.x + r.x, card.y + r.y, r.w, r.h);

    if !live {
        // One box with the last message or the prepare hint, and the load button under it.
        let message = shell.stem_errors[index]
            .clone()
            .or_else(|| {
                shell
                    .library
                    .as_ref()
                    .and_then(|panel| panel.stem_note(index))
            })
            .unwrap_or_else(|| "No stems for this track. Prepare stems in the library.".to_owned());
        parts.push(
            skin(place(l.stems), &look.fader.track)
                .flex()
                .justify_center()
                .p(px(12.))
                .text_color(paint(t.text_secondary))
                .text_size(px(t.type_body))
                .child(message),
        );
        let button = Rect::new(
            l.stems.x + 12,
            l.stems.bottom() - 44 - 12,
            (l.stems.w - 24).max(0),
            44,
        );
        parts.push(tile(
            t,
            button,
            "Load stems",
            (t.type_body, Tile::Idle),
            cx,
            move |shell: &mut Shell| shell.toggle_deck_stems(index),
        ));
        return parts;
    }

    let rows = stem_pad_layout(l.stems, STEM_ROW_GAP);
    let dragging = shell.drag.map(|d| d.target);
    for (stem, (state, pad)) in pads.stems().iter().zip(rows).enumerate() {
        let audible = !state.muted;
        let soloed = pads.soloed() == Some(stem);
        let mute_state = if audible {
            Tile::Lit(accent, t.on_accent)
        } else {
            Tile::Idle
        };
        parts.push(tile(
            t,
            pad.mute,
            STEM_NAMES[stem],
            (MIN_TEXT_SIZE, mute_state),
            cx,
            move |shell: &mut Shell| {
                shell.decks[index].stems.toggle_mute(stem);
            },
        ));
        // Activity glow: the name button fills in proportion to what the stem contributes.
        let level = pads.level(stem);
        if audible && level > 0.0 {
            let w = (pad.mute.w as f32 * level) as i32;
            parts.push(
                place(Rect::new(pad.mute.x, pad.mute.y, w, pad.mute.h))
                    .bg(accent_with_alpha(paint(t.on_accent), 0.25))
                    .rounded(px(t.control_radius)),
            );
        }
        let solo_state = if soloed {
            Tile::Lit(accent, t.on_accent)
        } else {
            Tile::Idle
        };
        parts.push(tile(
            t,
            pad.solo,
            "S",
            (MIN_TEXT_SIZE, solo_state),
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
        let mut filter = slider(
            t,
            pad.filter,
            (f64::from(state.filter), -1.0, 1.0),
            Target::StemFilter(index, stem),
            abs(pad.filter),
            cx,
        )
        .child(
            place(Rect::new(
                pad.filter.w / 2,
                5,
                1,
                (pad.filter.h - 10).max(1),
            ))
            .bg(paint(t.text_secondary.with_alpha(0.7))),
        );
        if dragging == Some(Target::StemFilter(index, stem)) {
            filter = filter.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(MIN_TEXT_SIZE))
                    .text_color(paint(t.text_primary))
                    .child(format!("{:+.2}", state.filter)),
            );
        }
        parts.push(filter);
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
                    let spectral = data.has_spectrum();
                    for i in 0..bar_count {
                        // One pixel column folds the points it covers, so the colour follows the
                        // spectrum under it.
                        let col = overview_column(points, i as usize, bar_count as usize);
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
                            paint(wave_colour(&col, spectral, &tokens)),
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

/// Pitch fader metrics (px): the readout strip on top, a 28 px thumb (the old one was 12) and an
/// 8 px track (was 4).
const PITCH_READOUT_H: i32 = 20;
const PITCH_THUMB_H: i32 = 28;
const PITCH_THUMB_W: i32 = 44;
const PITCH_TRACK_W: i32 = 8;

fn pitch(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.pitch;
    let range = deck.pitch.range();
    let value = deck.pitch.fader_value();
    // Top of the track is the fastest rate.
    let frac = ((1.0 + range - value) / (2.0 * range)).clamp(0.0, 1.0);
    let travel = (area.h - PITCH_READOUT_H - PITCH_THUMB_H).max(1);
    let thumb_y = PITCH_READOUT_H + (frac * f64::from(travel)) as i32;
    let centre_y = PITCH_READOUT_H + PITCH_THUMB_H / 2 + travel / 2;
    // The pointer position along this rectangle maps to the thumb's centre.
    let abs_pitch = Rect::new(
        card.x + area.x,
        card.y + area.y + PITCH_READOUT_H + PITCH_THUMB_H / 2,
        area.w,
        travel,
    );
    let mid_x = area.w / 2;

    place(area)
        .child(
            place(Rect::new(0, 0, area.w, PITCH_READOUT_H))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(t.type_label))
                .child(deck.pitch.readout()),
        )
        .child(
            place(Rect::new(
                mid_x - PITCH_TRACK_W / 2,
                PITCH_READOUT_H + PITCH_THUMB_H / 2,
                PITCH_TRACK_W,
                travel,
            ))
            .bg(paint(t.well))
            .rounded(px(PITCH_TRACK_W as f32 / 2.)),
        )
        // The 0 % mark, wider than the thumb so it shows beside it.
        .child(place(Rect::new(mid_x - 26, centre_y, 52, 2)).bg(paint(t.text_secondary)))
        .child(
            place(Rect::new(
                mid_x - PITCH_THUMB_W / 2,
                thumb_y,
                PITCH_THUMB_W,
                PITCH_THUMB_H,
            ))
            .bg(paint(t.surface_raised))
            .border_1()
            .border_color(paint(t.text_secondary))
            .rounded(px(t.control_radius))
            .child(
                place(Rect::new(6, PITCH_THUMB_H / 2 - 1, PITCH_THUMB_W - 14, 2))
                    .bg(paint(t.accent_for(index as i32))),
            ),
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
                let spectral = data.has_spectrum();
                let mut x = 0.0_f32;
                while x < w {
                    let frame = view.frame_at_x(f64::from(x));
                    let inside = frame < length as f64;
                    if let Some(col) = strip_column(level, frame, fpp).filter(|_| inside) {
                        let amp = col.peak * mid * 0.9;
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    bounds.origin.x + px(x),
                                    bounds.origin.y + px(mid - amp.max(1.0)),
                                ),
                                size(px(1.0), px((amp * 2.0).max(1.0))),
                            ),
                            paint(wave_colour(&col, spectral, &tokens)),
                        ));
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

/// Hotcue pads, two rows of four. A pad is its own label: its number sits inside, filled with the
/// cue's palette colour when the cue is set and dimmed when empty. Shift, Alt or Ctrl clears.
fn hotcues(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let pads = HotcuePads::layout(l.hotcues, CONTROL_GAP);
    let lit = deck.hotcues.lit();
    let mut row = place(Rect::sized(card.w, card.h));
    for (i, rect) in pads.iter().enumerate() {
        let cue = HotcuePads::colour(i);
        let (fill, text, border) = if lit[i] {
            (cue, ON_BRIGHT, cue)
        } else {
            (t.well, t.text_muted, t.border)
        };
        let pad = place(*rect)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(t.control_radius))
            .border_1()
            .border_color(paint(border))
            .bg(paint(fill))
            .text_color(paint(text))
            .text_size(px(t.type_title))
            .font_weight(gpui::FontWeight::BOLD)
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
        row = row.child(if lit[i] {
            pad
        } else {
            let hover = t.surface_hover;
            pad.hover(move |style| style.bg(paint(hover)))
        });
    }
    row
}

/// The loop row: halve, the size button, double. The size button shows the loop's length and is
/// the loop on/off switch: lit in the deck colour while looping, press to exit; idle it shows the
/// size a press would start.
fn loops(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let lay = LoopControls::layout(l.loops, CONTROL_GAP);
    let accent = t.accent_for(index as i32);
    let frame = deck.loops.frame();
    let size_state = if frame.reloop_lit {
        Tile::Lit(accent, t.on_accent)
    } else {
        Tile::Idle
    };
    place(Rect::sized(card.w, card.h))
        .child(tile(
            t,
            lay.halve,
            "\u{f7}2",
            (t.type_title, Tile::Idle),
            cx,
            move |this| {
                this.decks[index].loops.halve();
            },
        ))
        .child(tile(
            t,
            lay.size,
            &format!("LOOP {}", frame.label()),
            (t.type_label, size_state),
            cx,
            move |this| {
                this.decks[index].loops.toggle();
            },
        ))
        .child(tile(
            t,
            lay.double,
            "\u{d7}2",
            (t.type_title, Tile::Idle),
            cx,
            move |this| {
                this.decks[index].loops.double();
            },
        ))
}

/// The always-visible transport: Play (double width), Cue and SYNC, 44 px tall. SYNC is dark text
/// on the sync green while on.
fn transport(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &DeckLayout,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let rects = TransportPanel::layout(l.transport, CONTROL_GAP);
    let mut row = place(Rect::sized(card.w, card.h));
    for (i, button) in TransportButton::PRIMARY.into_iter().enumerate() {
        let label = match button {
            TransportButton::Play => deck.transport.play_label().to_uppercase(),
            other => other.label().to_owned(),
        };
        let state = if !deck.transport.is_lit(button) {
            Tile::Idle
        } else if button == TransportButton::Sync {
            Tile::Lit(SYNC_ON, ON_BRIGHT)
        } else {
            Tile::Lit(t.accent_for(index as i32), t.on_accent)
        };
        row = row.child(tile(
            t,
            rects[i],
            &label,
            (t.type_label, state),
            cx,
            move |this| {
                this.decks[index].transport.press(button);
            },
        ));
    }
    row
}

/// The tab strip under the drawer: Cues, Stems, More. The open one is lit in the deck colour.
fn tabs(shell: &Shell, index: usize, l: &DeckLayout, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let mode = shell.decks[index].pad_mode;
    let mut row = place(Rect::sized(card.w, card.h));
    for (mode_i, rect) in PadMode::ALL.into_iter().zip(tab_rects(l.tabs)) {
        let state = if mode_i == mode {
            Tile::Lit(t.accent_for(index as i32), t.on_accent)
        } else {
            Tile::Idle
        };
        row = row.child(tile(
            t,
            rect,
            mode_i.label(),
            (MIN_TEXT_SIZE, state),
            cx,
            move |this| {
                this.decks[index].pad_mode = mode_i;
            },
        ));
    }
    row
}

/// The More drawer: Master, key lock, quantize, slip and the stems on/off switch.
fn more(shell: &Shell, index: usize, l: &DeckLayout, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let accent = t.accent_for(index as i32);
    let cells = TransportPanel::secondary_layout(l.hotcues, CONTROL_GAP);
    let mut area = place(Rect::sized(card.w, card.h));
    for (button, rect) in TransportButton::SECONDARY.into_iter().zip(cells) {
        let state = if !deck.transport.is_lit(button) {
            Tile::Idle
        } else {
            Tile::Lit(button.lit_colour(t, index).into(), t.on_accent)
        };
        area = area.child(tile(
            t,
            rect,
            button.label(),
            (t.type_body, state),
            cx,
            move |this| {
                this.decks[index].transport.press(button);
            },
        ));
    }
    let live = stems_live(shell, index);
    area.child(tile(
        t,
        cells[4],
        if live { "Stems on" } else { "Stems off" },
        (
            t.type_body,
            if live {
                Tile::Lit(accent, t.on_accent)
            } else {
                Tile::Idle
            },
        ),
        cx,
        move |this| this.toggle_deck_stems(index),
    ))
}

/// A colour with its alpha replaced by `alpha` (GPUI keeps the RGB of the source colour).
fn accent_with_alpha(colour: gpui::Rgba, alpha: f32) -> gpui::Rgba {
    gpui::Rgba { a: alpha, ..colour }
}
