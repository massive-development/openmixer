//! One deck card: header, overview and zoomed waveforms, pitch fader, transport, hotcue pads, loop
//! row and a tab strip that swaps the pad area for the stems or the More toggles. Each part is drawn from its om-ui model; clicks call the model, which writes
//! through the string-id control surface. The waveforms are the analysed peaks the library loader
//! holds for the deck (none until the track has been analysed). A library track dropped on the card
//! loads into this deck.

use crate::app::Shell;
use crate::controls::slider;
use crate::drag::Target;
use crate::fader_paint;
use crate::library_view::TrackDragPayload;
use crate::paint::{paint, place, shadow, skin};
use crate::platter_paint::{
    angle_delta, paint_platter, platter_geometry, platter_rect, pointer_angle, scratch_target,
    zone_at, PlatterScene, Zone, SECONDS_PER_TURN,
};
use crate::waveform_paint::{paint_overview, paint_zoom, with_alpha, OverviewScene, ZoomScene};
use gpui::{
    canvas, div, prelude::*, px, relative, Bounds, Context, Div, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Render, SharedString, Stateful, Window,
};
use om_core::DeckSnapshot;
use om_ui::geometry::Rect;
use om_ui::stem_binding::{MAX_VOLUME, STEM_NAMES, STEM_SLIDER_LABELS};
use om_ui::style::LookAndFeel;
use om_ui::style::{contrast_ratio, over, relative_luminance};
use om_ui::theme::Colour;
use om_ui::theme::ThemeTokens;
use om_ui::waveform::{
    cue_flags, View, BAR_NUMBER_SIZE, CUE_FLAG_HEIGHT, CUE_FLAG_WIDTH, CUE_LINE_WIDTH,
    CUE_NUMBER_SIZE, CUE_STACK_STEP, CUE_TEXT_COLOUR, GRID_COLOUR, LOOP_COLOUR,
};
use om_ui::widgets::deck::{
    deck_controls_layout_for, deck_layout_for, tab_rects, DeckLayout, PadMode, CONTROL_GAP,
    STEM_ROW_GAP,
};
use om_ui::widgets::deck_header::deck_letter;
use om_ui::widgets::deck_header::{
    header_metrics, ARTIST_SIZE, BADGE_SIZE, ELAPSED_SIZE, MIN_TEXT_SIZE, ON_BRIGHT, SYNC_ON,
    TITLE_SIZE, WARN_RED,
};
use om_ui::widgets::hotcues::{HotcuePads, Modifiers};
use om_ui::widgets::loops::LoopFrame;
use om_ui::widgets::stage::STAGE_DECKS;
use om_ui::widgets::stem_pads::stem_pad_layout;
use om_ui::widgets::transport::{TransportButton, TransportPanel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Card for deck `index`, drawn at `card` (window coordinates).
pub fn deck_card(shell: &Shell, index: usize, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let drop_target = t.drop_target;
    // A stage deck's header, overview and zoomed waveform are drawn by the stage above.
    let area = Rect::sized(card.w, card.h);
    let mode = shell.decks[index].pad_mode;
    let snap = shell.engine.deck_snapshot(index).unwrap_or_default();
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
    let lay = local_layout(&l);
    let platter_on = platter_state(index).on;
    // The fader sits under the Platter switch.
    let pitch_layout = DeckLayout {
        pitch: lay.pitch,
        ..l.clone()
    };
    root = root.child(pitch(shell, index, card, &pitch_layout, cx));
    if !lay.platter_toggle.is_empty() {
        root = root.child(platter_switch(shell, index, lay.platter_toggle, cx));
    }
    if l.zoom_visible {
        root = root.child(zoom(shell, index, card, &l, cx));
    }
    root = root.child(transport(shell, index, card, &lay, &snap, cx));
    root = root.child(tabs(shell, index, &l, card, cx));
    if platter_on {
        return root.child(platter(shell, index, card, drawer_rect(&l), &snap, cx));
    }
    match mode {
        PadMode::Cues => {
            root = root.child(hotcues(shell, index, card, &lay, &snap, cx));
            if let Some(cells) = &lay.toggles {
                root = root.child(toggle_pills(shell, index, card, cells, &snap, cx));
            }
        }
        PadMode::More => root = root.child(more(shell, index, &l, card, &snap, cx)),
        PadMode::Stems => {
            for part in stems(shell, index, card, &l, cx) {
                root = root.child(part);
            }
        }
    }
    if !l.loops.is_empty() {
        root = root.child(loops(shell, index, card, &lay, &snap, cx));
    }
    root
}

/// How a [`tile`] is drawn.
#[derive(Clone, Copy)]
enum Tile {
    Idle,
    /// Lit: this fill with this text colour, and a glow.
    Lit(om_ui::theme::Colour, om_ui::theme::Colour),
    /// No track: idle colours at [`DISABLED_ALPHA`], no hover.
    Disabled,
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
        Tile::Idle | Tile::Disabled => (look.button.idle.fill, look.button.idle.text),
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
        Tile::Disabled => el.opacity(DISABLED_ALPHA),
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
        .child(cover(shell, index, l.header.h.min(COVER_MAX), &look))
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

/// Largest side of the cover tile in the deck header.
const COVER_MAX: i32 = 44;

/// The deck's cover art, square, or a token-coloured placeholder when the track has none (or its
/// image cannot be decoded: gpui then shows nothing inside the same tile).
fn cover(shell: &Shell, index: usize, side: i32, look: &LookAndFeel) -> Div {
    let t = &shell.theme;
    let tile = div()
        .flex_none()
        .w(px(side as f32))
        .h(px(side as f32))
        .overflow_hidden()
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(look.button.disabled.border))
        .bg(paint(look.button.disabled.fill));
    match shell.art_images.get(index).and_then(Option::as_ref) {
        Some(image) => tile.child(
            gpui::img(Arc::clone(image))
                .size_full()
                .object_fit(gpui::ObjectFit::Cover),
        ),
        None => tile,
    }
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
        parts.push(
            slider(
                t,
                pad.volume,
                (f64::from(state.gain), 0.0, MAX_VOLUME),
                Target::StemVolume(index, stem),
                abs(pad.volume),
                cx,
            )
            .child(slider_caption(t, STEM_SLIDER_LABELS[0])),
        );
        let mut filter = slider(
            t,
            pad.filter,
            (f64::from(state.filter), -1.0, 1.0),
            Target::StemFilter(index, stem),
            abs(pad.filter),
            cx,
        )
        .child(
            // The centre detent sits under the caption.
            place(Rect::new(
                pad.filter.w / 2,
                CAPTION_H,
                1,
                (pad.filter.h - CAPTION_H - 3).max(1),
            ))
            .bg(paint(t.text_secondary.with_alpha(0.7))),
        )
        .child(slider_caption(t, STEM_SLIDER_LABELS[1]));
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

/// Height of a stem slider's caption line; the filter's centre detent starts below it.
const CAPTION_H: i32 = 17;

/// A slider's name, top-left inside its track, 12 px, never taking a click.
fn slider_caption(t: &ThemeTokens, text: &str) -> Div {
    div()
        .absolute()
        .top(px(1.))
        .left(px(5.))
        .text_size(px(MIN_TEXT_SIZE))
        .text_color(paint(t.text_secondary))
        .child(text.to_owned())
}

/// The overview: the whole track in the three bands, played part dimmed, with the playhead, hot
/// cue ticks, loop and bar ticks. A click or a drag across it seeks.
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
    let origin_x = card.x + area.x;
    let scene = OverviewScene {
        deck: index,
        wave: shell.library.as_ref().and_then(|p| p.waveform(index)),
        markers: deck.overview.markers(width),
        well: om_ui::waveform::strip_well(t),
    };

    let canvas_el = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            paint_overview(window, bounds, &scene);
        },
    )
    .size_full();

    let seek = move |this: &mut Shell, x: f32| {
        this.decks[index]
            .overview
            .seek_at(f64::from(x - origin_x as f32), width);
    };
    place(area)
        .overflow_hidden()
        .bg(paint(om_ui::waveform::WAVE_WELL))
        .rounded(px(t.control_radius))
        .child(canvas_el)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                seek(this, f32::from(ev.position.x));
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
            if ev.dragging() {
                seek(this, f32::from(ev.position.x));
                cx.notify();
            }
        }))
}

/// A text label placed inside a strip.
fn label(x: f32, y: f32, size: f32, colour: gpui::Rgba, text: String) -> Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .text_size(px(size))
        .line_height(px(size + 2.0))
        .text_color(colour)
        .child(text)
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
    let height = area.h as f32;
    let view: View = deck.zoom.view(width);
    let grid = deck.zoom.grid_lines(&view);
    let loop_region = deck.zoom.loop_region(&view);
    let flags = cue_flags(&deck.zoom.cue_xs(&view), width);
    let scene = ZoomScene {
        deck: index,
        wave: shell.library.as_ref().and_then(|p| p.waveform(index)),
        view,
        length_frames: shell.engine.deck(index).length_frames(),
        grid: grid.clone(),
        loop_span: loop_region.as_ref().map(|r| (r.x0, r.x1, r.looping)),
        well: om_ui::waveform::strip_well(t),
    };

    let strip = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            paint_zoom(window, bounds, &scene);
        },
    )
    .size_full();

    let mut el = place(area)
        .overflow_hidden()
        .bg(paint(om_ui::waveform::WAVE_WELL))
        .rounded(px(t.control_radius))
        .child(strip);

    // Bar numbers sit at the foot of their line, in front of the waveform.
    let number_colour = with_alpha(GRID_COLOUR, 0.85);
    for line in &grid {
        if let Some(n) = line.bar_number {
            el = el.child(label(
                line.x as f32 + 4.0,
                height - BAR_NUMBER_SIZE - 6.0,
                BAR_NUMBER_SIZE,
                number_colour,
                n.to_string(),
            ));
        }
    }

    if let Some(region) = &loop_region {
        if !region.label.is_empty() {
            let x = (region.x0 as f32).max(0.0) + 6.0;
            el = el.child(label(
                x,
                height - BAR_NUMBER_SIZE - 6.0,
                BAR_NUMBER_SIZE,
                with_alpha(LOOP_COLOUR, region.edge_alpha()),
                region.label.clone(),
            ));
        }
    }

    // Hot cue flags: a 2 px line down the strip and a numbered tab at the top.
    for flag in &flags {
        el = el
            .child(
                div()
                    .absolute()
                    .left(px(flag.x))
                    .top(px(0.))
                    .w(px(CUE_LINE_WIDTH))
                    .h(px(height))
                    .bg(paint(flag.colour)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(flag.x))
                    .top(px(f32::from(flag.row) * CUE_STACK_STEP))
                    .w(px(CUE_FLAG_WIDTH))
                    .h(px(CUE_FLAG_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_br(px(3.))
                    .bg(paint(flag.colour))
                    .text_color(paint(CUE_TEXT_COLOUR))
                    .text_size(px(CUE_NUMBER_SIZE))
                    .child(flag.label.clone()),
            );
    }

    el.on_scroll_wheel(
        cx.listener(move |this, ev: &gpui::ScrollWheelEvent, _, cx| {
            let dy = f32::from(ev.delta.pixel_delta(px(16.)).y);
            // Both decks follow one time scale.
            let step = this.decks[index].zoom.wheel(dy);
            for deck in &mut this.decks {
                deck.zoom.set_zoom_step(step);
            }
            cx.notify();
        }),
    )
}

/// Pitch fader metrics (px): the readout strip on top, then the fader (cap, centre notch and
/// half-range ticks from `fader_paint`) at `PITCH_FADER_W` wide, centred in the column.
const PITCH_READOUT_H: i32 = 20;
const PITCH_FADER_W: i32 = 48;

fn pitch(shell: &Shell, index: usize, card: Rect, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let area = l.pitch;
    let range = deck.pitch.range();
    // Top of the track is the fastest rate.
    let pos = fader_paint::pitch_pos(deck.pitch.fader_value(), range);
    let w = PITCH_FADER_W.min(area.w);
    let local = Rect::new(
        (area.w - w) / 2,
        PITCH_READOUT_H,
        w,
        (area.h - PITCH_READOUT_H).max(1),
    );
    let abs = Rect::new(
        card.x + area.x + local.x,
        card.y + area.y + local.y,
        w,
        local.h,
    );
    place(area)
        .child(
            place(Rect::new(0, 0, area.w, PITCH_READOUT_H))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(t.type_label))
                .child(deck.pitch.readout()),
        )
        .child(fader_paint::fader(
            t,
            local,
            &fader_paint::FaderFace::pitch(),
            pos,
            t.accent_for(index as i32),
            Target::Pitch(index),
            abs,
            None,
            cx,
        ))
}

// ---------------------------------------------------------------------------------------------
// Transport, pads, loops and the platter. The decisions (which fill, which label, which rect) are
// the pure functions below, tested at the end of the file; the drawing functions only apply them.
// Research: docs/design/research/deck-controls.md sections 2 and 3.
// ---------------------------------------------------------------------------------------------

/// Diameter of the Play and Cue circles, and the floor they fall back to when the transport row is
/// shorter than 56 px (the 44 px touch floor).
pub(crate) const ROUND_DIAMETER: i32 = 56;
pub(crate) const ROUND_COMPACT: i32 = 44;
/// Gap between Play and Cue, and from Cue to Sync (research section 3).
const ROUND_GAP: i32 = 24;
const SYNC_GAP: i32 = 12;
const PILL_HEIGHT: i32 = 36;
const PILL_GAP: i32 = 8;
const PILL_RADIUS: f32 = 12.0;
/// Width and height of the Platter switch at the top of the pitch column.
const PLATTER_TOGGLE_W: i32 = 56;
const PLATTER_TOGGLE_H: i32 = 32;
/// Alpha of a set pad's fill; an active pad is opaque.
const PAD_ALPHA: f32 = 0.85;
const PAD_TAB_H: f32 = 3.0;
/// Daylight lit pads are darkened by this share so their text stays readable (research section 3).
const DAYLIGHT_DARKEN: f32 = 0.12;
/// Opacity of a control while it cannot act (no track loaded).
const DISABLED_ALPHA: f32 = 0.4;
/// Prefix of the active pad's label: a play triangle.
const ACTIVE_GLYPH: &str = "\u{25b6}";
/// Seconds after a cue point during which a playing deck marks that pad active. Unsourced.
const ACTIVE_WINDOW_S: f64 = 0.25;
/// Estimated width of one character at 12-14 px, to decide whether a toggle row fits.
const CHAR_W: i32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Round,
    Pill,
}

/// How one transport button is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ButtonStyle {
    pub shape: Shape,
    /// Nominal diameter of a round button (0 for pills); the layout may shrink it to the 44 floor.
    pub diameter: i32,
    pub fill: Colour,
    pub text: Colour,
    /// Glow colour while lit.
    pub glow: Option<Colour>,
    pub alpha: f32,
    pub hover: bool,
}

/// The style of a transport button. Idle is the `well` fill; lit toggles take the deck accent
/// (Sync keeps its own green, Play is green while playing, Cue is amber while held); with no track
/// the button is at 40 % opacity and has no hover or glow.
pub(crate) fn transport_style(
    t: &ThemeTokens,
    button: TransportButton,
    lit: bool,
    loaded: bool,
    deck: usize,
) -> ButtonStyle {
    let round = matches!(button, TransportButton::Play | TransportButton::Cue);
    let accent = t.accent_for(deck as i32);
    let look = LookAndFeel::from_tokens(t);
    let (fill, text, glow) = match (button, lit) {
        (_, false) => (look.toggle.off.fill, look.toggle.off.text, None),
        (TransportButton::Sync, true) => (SYNC_ON, text_on(t, SYNC_ON), Some(SYNC_ON)),
        (TransportButton::Play, true) => (t.meter_low, text_on(t, t.meter_low), Some(t.meter_low)),
        (TransportButton::Cue, true) => (t.warning, text_on(t, t.warning), Some(t.warning)),
        (_, true) => (look.toggle(true, accent).fill, look.lit_text, Some(accent)),
    };
    ButtonStyle {
        shape: if round { Shape::Round } else { Shape::Pill },
        diameter: if round { ROUND_DIAMETER } else { 0 },
        fill,
        text,
        glow: if loaded { glow } else { None },
        alpha: if loaded { 1.0 } else { DISABLED_ALPHA },
        hover: loaded && !lit,
    }
}

/// Text of a transport button (`om-ui` owns the words); Play reads Pause while playing.
pub(crate) fn transport_label(button: TransportButton, playing: bool) -> &'static str {
    match button {
        TransportButton::Play if playing => "Pause",
        other => other.label(),
    }
}

/// Tooltip text; Master's keeps its old name for people who search for it.
pub(crate) fn transport_tip(button: TransportButton) -> &'static str {
    button.tooltip()
}

/// How one hotcue pad is drawn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PadStyle {
    pub fill: Colour,
    pub text: Colour,
    pub border: Colour,
    pub dashed: bool,
    pub label: String,
    /// Colour tab along the top edge of a set pad.
    pub tab: Option<Colour>,
    pub glow: Option<Colour>,
    /// 1.0, or [`DISABLED_ALPHA`] while the deck has no track.
    pub alpha: f32,
    /// Whether the pad lightens under the pointer (never while disabled).
    pub hover: bool,
}

/// Opacity of any deck control: full, or [`DISABLED_ALPHA`] with no track loaded.
pub(crate) fn control_alpha(loaded: bool) -> f32 {
    if loaded {
        1.0
    } else {
        DISABLED_ALPHA
    }
}

/// What a pad sits on, for contrast: the well over the surface over the backdrop.
fn pad_ground(t: &ThemeTokens) -> Colour {
    over(t.well, over(t.surface, t.backdrop_bottom))
}

/// A daylight theme has a light surface.
fn is_light(t: &ThemeTokens) -> bool {
    relative_luminance(over(t.surface, t.backdrop_bottom)) > 0.5
}

/// The cue colour of pad `i` as it is filled: the palette, darkened 12 % on a light theme.
pub(crate) fn pad_colour(t: &ThemeTokens, i: usize) -> Colour {
    let c = HotcuePads::colour(i);
    if !is_light(t) {
        return c;
    }
    // Blend toward the near-black text colour: 12 % darker.
    Colour::mix(&[(c, 1.0 - DAYLIGHT_DARKEN), (ON_BRIGHT, DAYLIGHT_DARKEN)])
}

/// Dark or light text, whichever reads better on `fill` (WCAG contrast): the near-black bright-fill
/// ink, or the theme's primary or on-accent text. The research says black when luminance is above
/// 0.4; that threshold leaves mid-tone cue colours under 4.5:1, so the rule here is the crossover.
fn text_on(t: &ThemeTokens, fill: Colour) -> Colour {
    [ON_BRIGHT, t.text_primary, t.on_accent]
        .into_iter()
        .max_by(|a, b| contrast_ratio(*a, fill).total_cmp(&contrast_ratio(*b, fill)))
        .unwrap_or(ON_BRIGHT)
}

/// The look of pad `i`. Empty: dashed border, "Set", no colour. Set: its cue colour at 85 % with the
/// name and a 3 px tab. Active (playing from it): opaque with a glow.
pub(crate) fn pad_state(
    t: &ThemeTokens,
    set: bool,
    active: bool,
    i: usize,
    loaded: bool,
) -> PadStyle {
    if !set {
        return PadStyle {
            fill: t.well,
            text: t.text_secondary,
            border: t.border_strong,
            dashed: true,
            label: "Set".to_owned(),
            tab: None,
            glow: None,
            alpha: control_alpha(loaded),
            hover: loaded,
        };
    }
    let colour = pad_colour(t, i);
    let fill = colour.with_alpha(if active { 1.0 } else { PAD_ALPHA });
    let text = text_on(t, over(fill, pad_ground(t)));
    PadStyle {
        fill,
        text,
        // Active: an outline in the label ink (>= 4.5:1 on the fill) and a play glyph, so the state
        // does not rest on opacity and glow alone ("never rely on colour alone", States).
        border: if active { text } else { colour },
        dashed: false,
        label: if active {
            format!("{ACTIVE_GLYPH} Cue {}", i + 1)
        } else {
            format!("Cue {}", i + 1)
        },
        tab: Some(colour),
        glow: (active && loaded).then_some(colour),
        alpha: control_alpha(loaded),
        hover: false,
    }
}

/// Whether the deck is playing from the cue at `cue_frame`: within [`ACTIVE_WINDOW_S`] after it.
pub(crate) fn pad_active(s: &DeckSnapshot, cue_frame: Option<i64>) -> bool {
    match cue_frame {
        Some(c) if s.playing && s.file_sample_rate > 0.0 => {
            let d = (s.position_frames - c) as f64 / s.file_sample_rate;
            (0.0..ACTIVE_WINDOW_S).contains(&d)
        }
        _ => false,
    }
}

/// What the loop row shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoopView {
    pub label: String,
    /// The Loop button is lit while a loop is active.
    pub loop_lit: bool,
    pub halve_enabled: bool,
    pub double_enabled: bool,
}

pub(crate) fn loop_view(frame: &LoopFrame, loaded: bool) -> LoopView {
    LoopView {
        label: frame.label(),
        loop_lit: frame.reloop_lit,
        halve_enabled: loaded,
        double_enabled: loaded,
    }
}

/// Every rect the controls column places itself, from the shared [`DeckLayout`] (which this file
/// does not change). Card coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LocalLayout {
    pub play: Rect,
    pub cue: Rect,
    pub sync: Rect,
    /// Tempo leader, Key lock, Snap to beat, Slip as a row under the pads, when it fits.
    pub toggles: Option<[Rect; 4]>,
    /// The Platter switch at the top of the pitch column (empty when the column is too short).
    pub platter_toggle: Rect,
    /// The pitch column below the switch.
    pub pitch: Rect,
    /// The pads' area, shortened to leave room for the toggle row.
    pub hotcues: Rect,
    /// Halve, length chip, double, Loop switch.
    pub loops: [Rect; 4],
}

/// The union of the drawer's rects: where the pads, loops or stems (and so the platter) go.
pub(crate) fn drawer_rect(l: &DeckLayout) -> Rect {
    let parts: Vec<Rect> = [l.hotcues, l.loops, l.stems]
        .into_iter()
        .filter(|r| !r.is_empty())
        .collect();
    let Some(first) = parts.first() else {
        return Rect::default();
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.right(), first.bottom());
    for r in &parts {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.right());
        y1 = y1.max(r.bottom());
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

pub(crate) fn local_layout(l: &DeckLayout) -> LocalLayout {
    // Round buttons: 56 px, or the 44 px floor when the row is shorter. Play, Cue, then Sync.
    let tr = l.transport;
    let d = if tr.h >= ROUND_DIAMETER {
        ROUND_DIAMETER
    } else {
        ROUND_COMPACT.min(tr.h.max(0))
    };
    let y = tr.y + (tr.h - d) / 2;
    let gap = if tr.w >= d * 2 + ROUND_GAP + SYNC_GAP + 56 {
        ROUND_GAP
    } else {
        PILL_GAP
    };
    let play = Rect::new(tr.x, y, d, d);
    let cue = Rect::new(play.right() + gap, y, d, d);
    let sx = cue.right() + SYNC_GAP;
    let ph = PILL_HEIGHT.min(tr.h);
    let sync = Rect::new(sx, tr.y + (tr.h - ph) / 2, (tr.right() - sx).max(0), ph);

    // The Platter switch on top of the pitch column; the fader keeps the rest.
    let mut pitch_area = l.pitch;
    let platter_toggle = if pitch_area.h > PLATTER_TOGGLE_H * 3 {
        let top = pitch_area.remove_top(PLATTER_TOGGLE_H);
        pitch_area.remove_top(PILL_GAP);
        let w = PLATTER_TOGGLE_W.min(top.w);
        Rect::new(top.x + (top.w - w) / 2, top.y, w, top.h)
    } else {
        Rect::default()
    };

    // Pads, then a pill row of toggles if the spare height holds one and the labels fit.
    let mut hot = l.hotcues;
    let mut toggles = None;
    if !hot.is_empty() {
        let pads = HotcuePads::layout(hot, CONTROL_GAP);
        let used = pads[HotcuePads::COUNT - 1].bottom() - hot.y;
        if hot.h - used >= CONTROL_GAP + PILL_HEIGHT {
            let row = Rect::new(hot.x, hot.y + used + CONTROL_GAP, hot.w, PILL_HEIGHT);
            let cells = pill_row(row);
            let fits = cells
                .iter()
                .zip(TransportButton::SECONDARY)
                .all(|(r, b)| r.w >= transport_label(b, false).chars().count() as i32 * CHAR_W - 4);
            if fits {
                toggles = Some(cells);
                hot = Rect::new(hot.x, hot.y, hot.w, used);
            }
        }
    }

    LocalLayout {
        play,
        cue,
        sync,
        toggles,
        platter_toggle,
        pitch: pitch_area,
        hotcues: hot,
        loops: loop_rects(l.loops),
    }
}

/// Four pills across `row`, widths weighted by label length, `PILL_GAP` apart.
fn pill_row(row: Rect) -> [Rect; 4] {
    let weights =
        TransportButton::SECONDARY.map(|b| transport_label(b, false).chars().count() as i32 + 3);
    let total: i32 = weights.iter().sum();
    let room = row.w - PILL_GAP * 3;
    let mut x = row.x;
    let mut cells = [Rect::default(); 4];
    for (i, w) in weights.iter().enumerate() {
        let width = if i == 3 {
            row.right() - x
        } else {
            room * w / total
        };
        cells[i] = Rect::new(x, row.y, width, row.h);
        x += width + PILL_GAP;
    }
    cells
}

/// The loop row: halve, length chip, double (a stepper), then the Loop switch. Halve and double are
/// at least 32 px wide.
fn loop_rects(area: Rect) -> [Rect; 4] {
    let step_w = 36.min(area.w / 4);
    let switch_w = (area.w / 3).max(44).min(area.w);
    let stepper_w = (area.w - switch_w - CONTROL_GAP).max(0);
    let seam = 4;
    let chip_w = (stepper_w - step_w * 2 - seam * 2).max(0);
    let halve = Rect::new(area.x, area.y, step_w, area.h);
    let chip = Rect::new(halve.right() + seam, area.y, chip_w, area.h);
    let double = Rect::new(chip.right() + seam, area.y, step_w, area.h);
    let switch = Rect::new(area.right() - switch_w, area.y, switch_w, area.h);
    [halve, chip, double, switch]
}

// Platter mode and drag, per deck, held here so no shared model changes.
#[derive(Clone, Copy, Default)]
struct PlatterState {
    on: bool,
    scratch: Option<Scratch>,
}

#[derive(Clone, Copy)]
struct Scratch {
    start_frame: i64,
    start_angle: f32,
    last_angle: f32,
    total_angle: f32,
    was_playing: bool,
}

thread_local! {
    static PLATTERS: RefCell<HashMap<usize, PlatterState>> = RefCell::new(HashMap::new());
}

fn platter_state(index: usize) -> PlatterState {
    PLATTERS.with(|p| p.borrow().get(&index).copied().unwrap_or_default())
}

/// DEMO / TEST ONLY (`--demo-scene=platter`): switches the deck's platter on.
pub(crate) fn stage_platter(index: usize) {
    set_platter(index, |p| p.on = true);
}

fn set_platter(index: usize, f: impl FnOnce(&mut PlatterState)) {
    PLATTERS.with(|p| f(p.borrow_mut().entry(index).or_default()));
}

/// A tooltip bubble.
pub(crate) struct Tip {
    pub(crate) text: String,
    pub(crate) fill: Colour,
    pub(crate) ink: Colour,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(6.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(paint(self.fill))
            .text_color(paint(self.ink))
            .text_size(px(MIN_TEXT_SIZE))
            .child(self.text.clone())
    }
}

/// Text size of a transport button. A round button squeezed below its nominal 56 px (the 44 px
/// floor) takes the 12 px floor so the longest word, "Pause", keeps clear of the rim (about 10 px
/// spare at 44 px, against about 2 px at the 14 px body size). The 56 px circle and the pills keep
/// the body size. The spare-room figure is a measurement of the capture, not a sourced value.
pub(crate) fn button_text_size(t: &ThemeTokens, shape: Shape, rect: Rect) -> f32 {
    let body = t.type_body.max(MIN_TEXT_SIZE);
    if shape == Shape::Round && rect.w.min(rect.h) < ROUND_DIAMETER {
        MIN_TEXT_SIZE
    } else {
        body
    }
}

/// A transport button drawn from a [`ButtonStyle`]: a circle or a pill with a tooltip. `down` runs
/// on the left press, but not while the deck has no track.
fn transport_button(
    t: &ThemeTokens,
    rect: Rect,
    (label, tip): (&str, &'static str),
    style: ButtonStyle,
    cx: &mut Context<Shell>,
    down: impl Fn(&mut Shell) + 'static,
) -> Stateful<Div> {
    let look = LookAndFeel::from_tokens(t);
    let tip_style = look.tooltip;
    let radius = match style.shape {
        Shape::Round => rect.w.min(rect.h) as f32 / 2.0,
        Shape::Pill => PILL_RADIUS,
    };
    let el = place(rect)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .border_1()
        .border_color(paint(look.button.idle.border))
        .bg(paint(style.fill))
        .text_color(paint(style.text))
        .text_size(px(button_text_size(t, style.shape, rect)))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .opacity(style.alpha);
    let el = match style.glow {
        Some(c) => shadow(el, &look.glow(c)),
        None => el,
    };
    let el = if style.hover {
        let hover = t.surface_hover;
        el.hover(move |s| s.bg(paint(hover)))
    } else {
        el
    };
    el.child(label.to_owned())
        .id(SharedString::from(format!("tb-{}-{}", rect.x, rect.y)))
        .tooltip(move |_, cx| {
            cx.new(|_| Tip {
                text: tip.to_owned(),
                fill: tip_style.fill,
                ink: tip_style.text,
            })
            .into()
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                down(this);
                cx.notify();
            }),
        )
}

fn hotcues(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &LocalLayout,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let pads = HotcuePads::layout(l.hotcues, CONTROL_GAP);
    let lit = deck.hotcues.lit();
    let look = LookAndFeel::from_tokens(t);
    let mut row = place(Rect::sized(card.w, card.h));
    for (i, rect) in pads.iter().enumerate() {
        let style = pad_state(t, lit[i], pad_active(snap, snap.hotcues[i]), i, snap.loaded);
        let mut pad = place(*rect)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .border_1()
            .border_color(paint(style.border))
            .bg(paint(style.fill))
            .text_color(paint(style.text))
            .text_size(px(MIN_TEXT_SIZE))
            .font_weight(gpui::FontWeight::BOLD)
            .overflow_hidden();
        if style.dashed {
            pad = pad.border_dashed();
        }
        if let Some(c) = style.glow {
            pad = shadow(pad, &look.glow(c));
        }
        pad = pad.opacity(style.alpha);
        if style.hover {
            let hover = t.surface_hover;
            pad = pad.hover(move |s| s.bg(paint(hover)));
        }
        if let Some(tab) = style.tab {
            pad = pad.child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .w_full()
                    .h(px(PAD_TAB_H))
                    .bg(paint(tab)),
            );
        }
        row = row.child(pad.child(style.label.clone()).on_mouse_down(
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
        ));
    }
    row
}

/// The loop row: `[halve] [length] [double]` and a Loop switch. The length chip shows the loop's
/// size (or the size a press would start). Loop in and out are left out until the engine has them.
fn loops(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &LocalLayout,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let accent = t.accent_for(index as i32);
    let view = loop_view(&deck.loops.frame(), snap.loaded);
    let [halve, chip, double, switch] = l.loops;
    let step_state = |on: bool| if on { Tile::Idle } else { Tile::Disabled };
    let switch_state = if !snap.loaded {
        Tile::Disabled
    } else if view.loop_lit {
        Tile::Lit(accent, t.on_accent)
    } else {
        Tile::Idle
    };
    place(Rect::sized(card.w, card.h))
        .child(tile(
            t,
            halve,
            "\u{f7}2",
            (t.type_title, step_state(view.halve_enabled)),
            cx,
            move |this| {
                this.decks[index].loops.halve();
            },
        ))
        .child(
            place(chip)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(t.control_radius))
                .bg(paint(t.well))
                .border_1()
                .border_color(paint(t.border))
                .text_color(paint(t.text_primary))
                .text_size(px(t.type_body.max(MIN_TEXT_SIZE)))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .opacity(control_alpha(snap.loaded))
                .child(view.label.clone()),
        )
        .child(tile(
            t,
            double,
            "\u{d7}2",
            (t.type_title, step_state(view.double_enabled)),
            cx,
            move |this| {
                this.decks[index].loops.double();
            },
        ))
        .child(tile(
            t,
            switch,
            "Loop",
            (t.type_body, switch_state),
            cx,
            move |this| {
                this.decks[index].loops.toggle();
            },
        ))
}

/// The always-visible transport: round Play and Cue, then Sync as a pill. Play is green and reads
/// Pause while playing; Cue is amber while pressed.
fn transport(
    shell: &Shell,
    index: usize,
    card: Rect,
    l: &LocalLayout,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let cells = [l.play, l.cue, l.sync];
    let mut row = place(Rect::sized(card.w, card.h));
    for (button, rect) in TransportButton::PRIMARY.into_iter().zip(cells) {
        let lit = deck.transport.is_lit(button);
        let style = transport_style(t, button, lit, snap.loaded, index);
        let label = transport_label(button, snap.playing);
        let mut el = transport_button(
            t,
            rect,
            (label, transport_tip(button)),
            style,
            cx,
            move |this| {
                this.decks[index].transport.press(button);
            },
        );
        if button == TransportButton::Cue && snap.loaded {
            // Amber while the button is held.
            let amber = t.warning;
            let ink = ON_BRIGHT;
            el = el.active(move |s| s.bg(paint(amber)).text_color(paint(ink)));
        }
        row = row.child(el);
    }
    row
}

/// The tab strip under the drawer: Cues, Stems, More. The open one is lit in the deck colour; with
/// the platter open none is, and a tap closes the platter.
fn tabs(shell: &Shell, index: usize, l: &DeckLayout, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let mode = shell.decks[index].pad_mode;
    let platter = platter_state(index).on;
    let mut row = place(Rect::sized(card.w, card.h));
    for (mode_i, rect) in PadMode::ALL.into_iter().zip(tab_rects(l.tabs)) {
        let state = if mode_i == mode && !platter {
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
                set_platter(index, |p| p.on = false);
                this.decks[index].pad_mode = mode_i;
            },
        ));
    }
    row
}

/// The toggle pills: Tempo leader, Key lock, Snap to beat, Slip, at `cells`, lit in the deck colour.
fn toggle_pills(
    shell: &Shell,
    index: usize,
    card: Rect,
    cells: &[Rect],
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let mut area = place(Rect::sized(card.w, card.h));
    for (button, rect) in TransportButton::SECONDARY.into_iter().zip(cells) {
        let style = transport_style(t, button, deck.transport.is_lit(button), snap.loaded, index);
        area = area.child(transport_button(
            t,
            *rect,
            (transport_label(button, false), transport_tip(button)),
            style,
            cx,
            move |this| {
                this.decks[index].transport.press(button);
            },
        ));
    }
    area
}

/// The More drawer when the pills do not fit under the pads: the same toggles in a grid, plus the
/// stems switch.
fn more(
    shell: &Shell,
    index: usize,
    l: &DeckLayout,
    card: Rect,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let accent = t.accent_for(index as i32);
    let cells = TransportPanel::secondary_layout(l.hotcues, CONTROL_GAP);
    let area = toggle_pills(shell, index, card, &cells[..4], snap, cx);
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

/// The Platter switch at the top of the pitch column.
fn platter_switch(
    shell: &Shell,
    index: usize,
    rect: Rect,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let t = &shell.theme;
    let on = platter_state(index).on;
    let accent = t.accent_for(index as i32);
    let track = LookAndFeel::from_tokens(t).toggle(on, accent);
    let style = ButtonStyle {
        shape: Shape::Pill,
        diameter: 0,
        fill: track.fill,
        text: track.text,
        glow: on.then_some(accent),
        alpha: 1.0,
        hover: !on,
    };
    transport_button(
        t,
        rect,
        ("Platter", "Show the platter: drag the disc to scrub"),
        style,
        cx,
        move |_| set_platter(index, |p| p.on = !p.on),
    )
}

/// The platter, in the place of the pads and loops: the disc, the played arc on its ring, the
/// turning marker and ticks, and the static deck label in the middle. Dragging the inner 85 %
/// scrubs through `Seek` (best effort, see the plan's risk 2); the rim band is painted but does
/// nothing yet.
fn platter(
    shell: &Shell,
    index: usize,
    card: Rect,
    area: Rect,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let rect = platter_rect(area);
    let geometry = platter_geometry(Rect::new(card.x + rect.x, card.y + rect.y, rect.w, rect.h));
    let accent = t.accent_for(index as i32);
    let seconds = if snap.file_sample_rate > 0.0 {
        shell.decks[index].zoom.playhead_frame() as f64 / snap.file_sample_rate
    } else {
        0.0
    };
    let scene = PlatterScene {
        deck: index,
        accent,
        disc: t.surface_raised,
        track: t.border,
        groove: t.border_strong.with_alpha(0.5),
        tick: t.text_muted,
        marker: t.text_primary,
        position_frames: snap.position_frames,
        length_frames: snap.length_frames,
        track_seconds: seconds,
    };
    let canvas_el = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            paint_platter(window, bounds, &scene);
        },
    )
    .size_full();

    let label_d = (geometry.label_radius * 2.0).round() as i32;
    let letter = deck_letter(index).to_string();
    let label = place(Rect::new(
        (rect.w - label_d) / 2,
        (rect.h - label_d) / 2,
        label_d,
        label_d,
    ))
    .flex()
    .items_center()
    .justify_center()
    .rounded(px(label_d as f32 / 2.0))
    .bg(paint(accent))
    .text_color(paint(t.on_accent))
    .text_size(px((label_d as f32 * 0.5).max(MIN_TEXT_SIZE)))
    .font_weight(gpui::FontWeight::BOLD)
    .child(letter);

    let loaded = snap.loaded;
    let length = snap.length_frames;
    let per_turn = platter_frames_per_turn(snap.file_sample_rate);
    let at = |ev_x: gpui::Pixels, ev_y: gpui::Pixels| (f32::from(ev_x), f32::from(ev_y));
    let down = cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
        let p = at(ev.position.x, ev.position.y);
        if loaded && zone_at(p, &geometry) == Zone::Scratch {
            let state = this.engine.deck_snapshot(index).unwrap_or_default();
            let angle = pointer_angle(p, &geometry);
            // Touching the platter stops the motor; releasing starts it again.
            if state.playing {
                this.decks[index].transport.press(TransportButton::Play);
            }
            set_platter(index, |s| {
                s.scratch = Some(Scratch {
                    start_frame: state.position_frames,
                    start_angle: angle,
                    last_angle: angle,
                    total_angle: angle,
                    was_playing: state.playing,
                })
            });
            cx.notify();
        }
    });
    let moved = cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
        let Some(mut s) = platter_state(index).scratch else {
            return;
        };
        if !ev.dragging() {
            return;
        }
        let angle = pointer_angle(at(ev.position.x, ev.position.y), &geometry);
        s.total_angle += angle_delta(s.last_angle, angle);
        s.last_angle = angle;
        set_platter(index, |p| p.scratch = Some(s));
        let target = scratch_target(
            s.start_frame,
            s.start_angle,
            s.total_angle,
            per_turn,
            length,
        );
        if length > 0 {
            // `Seek` takes a fraction of the track; `seek_at(x, 1.0)` sends x clamped to 0..1.
            this.decks[index]
                .overview
                .seek_at(target as f64 / length as f64, 1.0);
        }
        cx.notify();
    });
    let release = cx.listener(move |this, _: &gpui::MouseUpEvent, _, cx| {
        end_scratch(this, index);
        cx.notify();
    });
    let release_out = cx.listener(move |this, _: &gpui::MouseUpEvent, _, cx| {
        end_scratch(this, index);
        cx.notify();
    });

    place(rect)
        .child(canvas_el)
        .child(label)
        .on_mouse_down(MouseButton::Left, down)
        .on_mouse_move(moved)
        .on_mouse_up(MouseButton::Left, release)
        .on_mouse_up_out(MouseButton::Left, release_out)
}

/// Ends a scratch: the deck plays again if it was playing when the platter was touched.
fn end_scratch(shell: &mut Shell, index: usize) {
    if let Some(s) = platter_state(index).scratch {
        set_platter(index, |p| p.scratch = None);
        if s.was_playing {
            shell.decks[index].transport.press(TransportButton::Play);
        }
    }
}

/// Track frames per marker turn: the same 1.8 s the disc turns in.
fn platter_frames_per_turn(file_sample_rate: f64) -> f64 {
    SECONDS_PER_TURN * file_sample_rate
}

/// A colour with its alpha replaced by `alpha` (GPUI keeps the RGB of the source colour).
fn accent_with_alpha(colour: gpui::Rgba, alpha: f32) -> gpui::Rgba {
    gpui::Rgba { a: alpha, ..colour }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::theme::{parse_theme, BUILTIN_THEMES};
    use om_ui::widgets::deck::deck_controls_layout_for;

    fn theme(id: &str) -> ThemeTokens {
        let (_, json) = BUILTIN_THEMES
            .iter()
            .find(|(file, _)| file.contains(id))
            .expect("built-in theme");
        parse_theme(json).expect("theme parses").tokens
    }

    fn themes() -> [ThemeTokens; 2] {
        [theme("dark"), theme("daylight")]
    }

    #[test]
    fn play_and_cue_are_56_px_circles_and_the_toggles_are_pills() {
        let t = theme("dark");
        for b in [TransportButton::Play, TransportButton::Cue] {
            let s = transport_style(&t, b, false, true, 0);
            assert_eq!((s.shape, s.diameter), (Shape::Round, 56), "{b:?}");
        }
        for b in [
            TransportButton::Sync,
            TransportButton::Master,
            TransportButton::KeyLock,
            TransportButton::Quantize,
            TransportButton::Slip,
        ] {
            assert_eq!(transport_style(&t, b, false, true, 0).shape, Shape::Pill);
        }
    }

    #[test]
    fn lit_transport_labels_read_at_4_5_to_1_in_both_themes() {
        // The daylight Play green is dark: near-black ink on it measured 2.5:1 in the staged
        // `cues-loop-daylight` capture. The ink is now the better of the candidates (labels-layout.md
        // item 5: text 4.5:1).
        for t in themes() {
            for b in [
                TransportButton::Play,
                TransportButton::Cue,
                TransportButton::Sync,
            ] {
                let s = transport_style(&t, b, true, true, 0);
                assert!(
                    contrast_ratio(s.text, s.fill) >= 4.5,
                    "{b:?}: {}",
                    contrast_ratio(s.text, s.fill)
                );
            }
        }
    }

    #[test]
    fn lit_states_use_the_documented_fills() {
        let t = theme("dark");
        let play = transport_style(&t, TransportButton::Play, true, true, 1);
        assert_eq!(play.fill, t.meter_low);
        assert!(play.glow.is_some());
        assert_eq!(transport_label(TransportButton::Play, true), "Pause");
        assert_eq!(transport_label(TransportButton::Play, false), "Play");
        assert_eq!(
            transport_style(&t, TransportButton::Cue, true, true, 0).fill,
            t.warning
        );
        assert_eq!(
            transport_style(&t, TransportButton::Sync, true, true, 0).fill,
            SYNC_ON
        );
        let slip = transport_style(&t, TransportButton::Slip, true, true, 2);
        assert_eq!(slip.fill, t.accent_for(2));
        let idle = transport_style(&t, TransportButton::Slip, false, true, 2);
        assert_eq!(idle.fill, t.well);
        assert!(idle.glow.is_none() && idle.hover);
    }

    #[test]
    fn a_deck_without_a_track_is_dimmed_with_no_hover_or_glow() {
        let t = theme("dark");
        for b in TransportButton::ALL {
            let s = transport_style(&t, b, true, false, 0);
            assert_eq!(s.alpha, 0.4);
            assert!(!s.hover && s.glow.is_none(), "{b:?}");
        }
    }

    #[test]
    fn pads_and_the_loop_row_dim_with_no_track_like_the_transport() {
        // deck-controls.md States: "Disabled (no track): 40% opacity, no hover, no pointer".
        for t in themes() {
            for (set, active) in [(false, false), (true, false), (true, true)] {
                let off = pad_state(&t, set, active, 1, false);
                assert_eq!(off.alpha, DISABLED_ALPHA);
                assert!(!off.hover && off.glow.is_none());
                let on = pad_state(&t, set, active, 1, true);
                assert_eq!(on.alpha, 1.0);
            }
        }
        assert_eq!(control_alpha(false), 0.4);
        assert_eq!(control_alpha(true), 1.0);
    }

    #[test]
    fn pause_keeps_clear_of_the_rim_of_a_squeezed_round_button() {
        // deck-controls.md Buttons: Play is a 56 px circle with icon + word; the build falls to the
        // 44 px floor when the row is tight, where the body-size word nearly touched the rim.
        let t = theme("dark");
        let small = Rect::new(0, 0, 44, 44);
        let full = Rect::new(0, 0, 56, 56);
        assert_eq!(button_text_size(&t, Shape::Round, small), MIN_TEXT_SIZE);
        assert_eq!(button_text_size(&t, Shape::Round, full), t.type_body);
        assert_eq!(button_text_size(&t, Shape::Pill, small), t.type_body);
        // The word is unchanged.
        assert_eq!(transport_label(TransportButton::Play, true), "Pause");
    }

    #[test]
    fn words_are_plain_and_master_stays_findable() {
        assert_eq!(
            transport_label(TransportButton::Master, false),
            "Tempo leader"
        );
        assert_eq!(
            transport_label(TransportButton::Quantize, false),
            "Snap to beat"
        );
        assert!(transport_tip(TransportButton::Master).contains("master"));
    }

    #[test]
    fn empty_pads_are_dashed_and_say_set_with_no_colour() {
        let t = theme("dark");
        let p = pad_state(&t, false, false, 3, true);
        assert!(p.dashed);
        assert_eq!(p.label, "Set");
        assert!(p.tab.is_none() && p.glow.is_none());
        assert_eq!(p.fill, t.well);
    }

    #[test]
    fn set_pads_take_the_cue_colour_at_85_percent_with_a_tab() {
        let t = theme("dark");
        let p = pad_state(&t, true, false, 2, true);
        assert!(!p.dashed);
        assert_eq!(p.label, "Cue 3");
        assert!((p.fill.alpha() - 0.85).abs() < 0.01);
        assert_eq!(p.tab, Some(HotcuePads::colour(2)));
        assert!(p.glow.is_none());
        let active = pad_state(&t, true, true, 2, true);
        assert!((active.fill.alpha() - 1.0).abs() < 0.01);
        assert!(active.glow.is_some());
    }

    #[test]
    fn the_active_pad_changes_label_and_outline_not_just_opacity_and_glow() {
        // deck-controls.md States: "Active: full fill plus glow" and "Never rely on colour alone:
        // label text and glyph change with state".
        for t in themes() {
            let set = pad_state(&t, true, false, 2, true);
            let active = pad_state(&t, true, true, 2, true);
            assert_ne!(active.label, set.label);
            assert!(active.label.contains("Cue 3") && active.label.starts_with('\u{25b6}'));
            assert_eq!(active.border, active.text);
            assert_ne!(active.border, set.border);
            let fill = over(active.fill, pad_ground(&t));
            assert!(contrast_ratio(active.border, fill) >= 3.0);
        }
    }

    #[test]
    fn pad_text_reads_at_4_5_to_1_on_every_cue_colour_in_both_themes() {
        for t in themes() {
            for i in 0..8 {
                for active in [false, true] {
                    let p = pad_state(&t, true, active, i, true);
                    let ground = over(p.fill, pad_ground(&t));
                    let ratio = contrast_ratio(p.text, ground);
                    assert!(ratio >= 4.5, "pad {i} active {active}: {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn daylight_darkens_the_cue_colours_and_dark_does_not() {
        let [dark, light] = themes();
        assert_eq!(pad_colour(&dark, 0), HotcuePads::colour(0));
        assert!(
            relative_luminance(pad_colour(&light, 2)) < relative_luminance(HotcuePads::colour(2))
        );
    }

    #[test]
    fn a_pad_is_active_only_while_playing_just_after_its_cue() {
        let s = DeckSnapshot {
            playing: true,
            file_sample_rate: 1000.0,
            position_frames: 1100,
            ..DeckSnapshot::default()
        };
        assert!(pad_active(&s, Some(1000)));
        assert!(!pad_active(&s, Some(2000)), "cue ahead");
        assert!(!pad_active(&s, Some(100)), "long passed");
        assert!(!pad_active(&s, None));
        let paused = DeckSnapshot {
            playing: false,
            ..s
        };
        assert!(!pad_active(&paused, Some(1000)));
    }

    #[test]
    fn loop_view_shows_the_length_and_lights_with_the_loop() {
        let frame = LoopFrame {
            size_lit: [false; 8],
            reloop_lit: true,
            length_beats: 8.0,
        };
        let v = loop_view(&frame, true);
        assert_eq!(v.label, frame.label());
        assert!(v.loop_lit && v.halve_enabled && v.double_enabled);
        let idle = LoopFrame {
            reloop_lit: false,
            length_beats: 0.0,
            ..frame
        };
        assert!(!loop_view(&idle, true).loop_lit);
        assert!(!loop_view(&idle, false).halve_enabled);
    }

    fn controls(w: i32, h: i32, mode: PadMode) -> DeckLayout {
        deck_controls_layout_for(Rect::sized(w, h), &theme("dark"), mode)
    }

    fn all_rects(l: &DeckLayout, lay: &LocalLayout) -> Vec<Rect> {
        let mut v = vec![lay.play, lay.cue, lay.sync, lay.pitch, lay.hotcues, l.tabs];
        v.extend(lay.loops);
        if !lay.platter_toggle.is_empty() {
            v.push(lay.platter_toggle);
        }
        if let Some(cells) = lay.toggles {
            v.extend(cells);
        }
        v.retain(|r| !r.is_empty());
        v
    }

    #[test]
    fn local_rects_stay_inside_the_card_and_never_overlap_at_any_size() {
        for (w, h) in [(290, 262), (290, 330), (560, 420), (400, 700)] {
            let card = Rect::sized(w, h);
            let l = controls(w, h, PadMode::Cues);
            let lay = local_layout(&l);
            let rects = all_rects(&l, &lay);
            for (i, a) in rects.iter().enumerate() {
                assert!(card.contains(a), "{w}x{h}: {a:?} escapes");
                for b in &rects[i + 1..] {
                    assert!(!a.intersects(b), "{w}x{h}: {a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn round_buttons_are_56_px_when_the_row_allows_and_never_below_44() {
        let mut l = controls(290, 262, PadMode::Cues);
        l.transport = Rect::new(10, 10, 230, 60);
        let lay = local_layout(&l);
        assert_eq!((lay.play.w, lay.play.h), (56, 56));
        assert_eq!(lay.cue.x - lay.play.right(), 24);
        assert_eq!(lay.sync.x - lay.cue.right(), 12);
        l.transport = Rect::new(10, 10, 230, 44);
        let lay = local_layout(&l);
        assert_eq!((lay.play.w, lay.play.h), (44, 44));
        assert!(lay.sync.h >= 32 && lay.sync.w >= 44, "{:?}", lay.sync);
    }

    #[test]
    fn halve_and_double_keep_a_32_px_hit_target() {
        let l = controls(290, 262, PadMode::Cues);
        let lay = local_layout(&l);
        assert!(lay.loops[0].w >= 32 && lay.loops[2].w >= 32);
        assert!(lay.loops[3].w >= 44 && lay.loops.iter().all(|r| r.h >= 32));
    }

    #[test]
    fn the_toggle_row_appears_only_when_36_px_of_height_is_spare() {
        let tall = local_layout(&controls(420, 700, PadMode::Cues));
        assert!(tall.toggles.is_some(), "{tall:?}");
        let short = local_layout(&controls(290, 262, PadMode::Cues));
        assert!(short.toggles.is_none());
        // Without the row the pads keep the whole area.
        assert_eq!(short.hotcues, controls(290, 262, PadMode::Cues).hotcues);
    }

    #[test]
    fn the_platter_drawer_covers_pads_and_loops_without_touching_the_transport() {
        let l = controls(290, 262, PadMode::Cues);
        let d = drawer_rect(&l);
        assert!(d.contains(&l.hotcues) && d.contains(&l.loops));
        assert!(!d.intersects(&l.transport) && !d.intersects(&l.tabs));
        let s = controls(290, 262, PadMode::Stems);
        assert!(drawer_rect(&s).contains(&s.stems));
    }
}
