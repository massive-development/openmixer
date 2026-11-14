//! One deck on the faceplate (comp 4a/4b): its header in the deck header row (tag, cover, title and
//! artist, BPM, key, remaining and the overview strip), its zoomed waveform on the stage, and its
//! controls in the controls row: the pitch fader at the outside edge, Play and Cue, Sync and Key
//! lock, the Hot cues / Loop / Stems / FX tabs and what they open. Deck B is drawn mirrored so each
//! deck reads from its outside edge. Each part is drawn from its om-ui model; presses call the
//! model, which writes through the control surface. A library track dropped on a deck's header or
//! controls loads into that deck.

use crate::app::Shell;
use crate::controls::{
    face_button, metrics, pad_button, press_button, segmented, slider, slider_focus,
};
use crate::drag::Target;
use crate::fader_paint;
use crate::font::{numeral, Numeral, TypeExt};
use crate::library_view::TrackDragPayload;
use crate::paint::{paint, place};
use crate::parts::{deck_tag, state_word, PadState};
use crate::platter_paint::{
    angle_delta, paint_platter, platter_geometry, platter_rect, pointer_angle, scratch_target,
    zone_at, PlatterScene, Zone, SECONDS_PER_TURN,
};
use crate::waveform_paint::{
    paint_overview, paint_zoom, style as wave_style, OverviewScene, ZoomScene,
};
use gpui::{
    canvas, div, prelude::*, px, relative, Bounds, Context, Div, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Render, Window,
};
use om_core::DeckSnapshot;
use om_ui::geometry::Rect;
use om_ui::main_layout::is_mirrored;
use om_ui::stem_binding::{MAX_VOLUME, STEM_NAMES};
use om_ui::style::{Face, LookAndFeel};
use om_ui::theme::{Colour, ThemeTokens, TypeRole};
use om_ui::waveform::CUE_STACK_STEP;
use om_ui::waveform::{
    cue_flags, StripInk, View, BAR_NUMBER_SIZE, CUE_FLAG_HEIGHT, CUE_FLAG_WIDTH,
};
use om_ui::widgets::deck::{
    deck_controls_layout, pad_grid, platter_layout, DeckLayout, PadMode, PAD_COUNT,
};
use om_ui::widgets::deck_header::{deck_letter, head_layout, TITLE_LINE};
use om_ui::widgets::hotcues::Modifiers;
use om_ui::widgets::loops::{LoopFrame, SIZES, SIZE_NAMES};
use om_ui::widgets::mixer::Page;
use om_ui::widgets::stem_pads::stem_pad_layout;
use om_ui::widgets::transport::TransportButton;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Seconds after a cue point during which a playing deck shows that pad as playing. Unsourced.
const ACTIVE_WINDOW_S: f64 = 0.25;

/// The deck's hue (its letter's colour).
fn hue(t: &ThemeTokens, index: usize) -> Colour {
    t.accent_for(index as i32)
}

/// A library track dropped on `el` loads into deck `index`.
fn load_on_drop(el: Div, t: &ThemeTokens, index: usize, cx: &mut Context<Shell>) -> Div {
    let drop_target = t.drop_target;
    el.drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(paint(drop_target)))
        .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
            if let Some(panel) = this.library.as_mut() {
                let _ = panel.drop_on_deck(index, &payload.description);
            }
            cx.notify();
        }))
}

// ---- The deck header -------------------------------------------------------------------------

/// Deck `index`'s header cell at `cell` (window coordinates): tag, cover, title and artist, the
/// readouts and the overview strip under them; deck B mirrored.
pub fn head_card(shell: &Shell, index: usize, cell: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let mirrored = is_mirrored(index);
    let l = head_layout(Rect::sized(cell.w, cell.h), t, &m, mirrored);
    let text = shell.decks[index].header.text();
    let look = LookAndFeel::from_tokens(t);
    let hue = hue(t, index);
    let title_block = div()
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(m.space[1]))
        .overflow_hidden()
        .when(mirrored, |el| el.items_end())
        .child(
            div()
                .type_role(t, TypeRole::Body)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .line_height(relative(TITLE_LINE))
                .truncate()
                .child(text.title),
        )
        .child(
            div()
                .type_role(t, TypeRole::Body)
                .line_height(relative(TITLE_LINE))
                .text_color(paint(t.text_secondary))
                .truncate()
                .child(text.artist),
        );
    let pitch = shell.decks[index].pitch.readout();
    let remaining_caption = if text.remaining_warn {
        "Ending soon"
    } else {
        "Remaining"
    };
    let readouts = [
        (text.bpm, look.readout.value, Some(pitch), "BPM"),
        (text.key, look.readout.value, None, "Key"),
        (text.remaining, hue, None, remaining_caption),
    ];
    let mut el = place(cell)
        .text_color(paint(t.text_primary))
        .child(place(l.tag).child(deck_tag(t, &text.letter.to_string(), hue)))
        .child(cover(shell, index, l.cover))
        .child(
            place(l.title)
                .flex()
                .items_center()
                .when(mirrored, |e| e.justify_end())
                .child(title_block),
        );
    for (rect, (value, colour, unit, caption)) in l.readouts.into_iter().zip(readouts) {
        el = el.child(readout(t, rect, (value, colour), (caption, unit), mirrored));
    }
    let el = el.child(overview(shell, index, cell, l.overview, cx));
    load_on_drop(el, t, index, cx)
}

/// One readout: the value in the readout numeral over its caption in the label tier, aligned to
/// the inner edge (deck A's right, deck B's left).
fn readout(
    t: &ThemeTokens,
    rect: Rect,
    (value, colour): (String, Colour),
    (caption, unit): (&str, Option<String>),
    mirrored: bool,
) -> Div {
    let m = metrics(t);
    let mut caption_row = div()
        .flex()
        .items_baseline()
        .gap(px(m.space[0]))
        .type_role(t, TypeRole::Label)
        .text_color(paint(t.text_muted))
        .child(caption.to_owned());
    if let Some(unit) = unit {
        caption_row = caption_row.child(numeral(t, Numeral::Value, unit));
    }
    place(rect)
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(m.space[1]))
        .when(!mirrored, |el| el.items_end())
        .when(mirrored, |el| el.items_start())
        .whitespace_nowrap()
        .child(numeral(t, Numeral::Readout, value).text_color(paint(colour)))
        .child(caption_row)
}

/// The deck's cover art at `rect`; nothing when the track has none (the comp's header has no
/// tile of its own).
fn cover(shell: &Shell, index: usize, rect: Rect) -> Div {
    let t = &shell.theme;
    match shell.art_images.get(index).and_then(Option::as_ref) {
        Some(image) => place(rect)
            .overflow_hidden()
            .rounded(px(t.control_radius))
            .border_1()
            .border_color(paint(t.border))
            .child(
                gpui::img(Arc::clone(image))
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover),
            ),
        None => place(rect),
    }
}

/// The overview at `rect` (cell coordinates; `cell` is the header cell in the window): the whole
/// track in the deck colour, the played part shaded, the playhead and the cue and loop marks. A
/// click or a drag across it seeks.
pub(crate) fn overview(
    shell: &Shell,
    index: usize,
    cell: Rect,
    rect: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let width = f64::from(rect.w);
    let origin_x = cell.x + rect.x;
    let ink = StripInk::new(t, wave_style(), index);
    let scene = OverviewScene {
        deck: index,
        wave: shell.library.as_ref().and_then(|p| p.waveform(index)),
        markers: deck.overview.markers(width),
        ink,
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
    place(rect)
        .overflow_hidden()
        .bg(paint(ink.well))
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

/// The master tempo over the mixer column: the tempo the mix runs at, two decimals, captioned.
pub fn tempo_card(shell: &Shell, cell: Rect) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let bpm = om_ui::widgets::deck::format_tempo(shell.master_bpm());
    place(cell)
        .flex()
        .flex_col()
        .items_center()
        .justify_end()
        .gap(px(m.space[1]))
        .child(numeral(t, Numeral::Readout, bpm).text_color(paint(t.readout)))
        .child(
            div()
                .type_role(t, TypeRole::Label)
                .text_color(paint(t.text_muted))
                .child("Master tempo"),
        )
}

// ---- The zoomed waveform ---------------------------------------------------------------------

/// How much of the well shows through the plate behind a strip's text. Unsourced: enough that a
/// waveform peak or the playhead under the text does not break its letters.
const PLATE_ALPHA: f32 = 0.8;

/// A number placed inside a strip (a bar number), on a plate of the well so peaks do not cross it.
fn strip_number(t: &ThemeTokens, x: f32, y: f32, ink: &StripInk, text: String) -> Div {
    numeral(t, Numeral::Value, text)
        .absolute()
        .left(px(x))
        .top(px(y))
        .px(px(metrics(t).space[0]))
        .bg(paint(ink.well.with_alpha(PLATE_ALPHA)))
        .text_color(paint(ink.number))
}

/// Deck `index`'s zoomed waveform at `area` (stage coordinates): the strip, bar numbers, the loop
/// and its length, the cue flags and the playhead. The wheel zooms both decks together.
pub(crate) fn zoom(shell: &Shell, index: usize, area: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let width = f64::from(area.w);
    let height = area.h as f32;
    let view: View = deck.zoom.view(width);
    let grid = deck.zoom.grid_lines(&view);
    let loop_region = deck.zoom.loop_region(&view);
    let flags = cue_flags(&deck.zoom.cue_xs(&view), width);
    let ink = StripInk::new(t, wave_style(), index);
    let scene = ZoomScene {
        deck: index,
        wave: shell.library.as_ref().and_then(|p| p.waveform(index)),
        view,
        length_frames: shell.engine.deck(index).length_frames(),
        grid: grid.clone(),
        loop_span: loop_region.as_ref().map(|r| (r.x0, r.x1, r.looping)),
        ink,
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
        .bg(paint(ink.well))
        .child(strip);
    for line in &grid {
        if let Some(n) = line.bar_number {
            el = el.child(strip_number(
                t,
                line.x as f32 + 1.0,
                height - BAR_NUMBER_SIZE - 6.0,
                &ink,
                n.to_string(),
            ));
        }
    }
    if let Some(region) = &loop_region {
        let caption = region.caption();
        if !caption.is_empty() {
            el = el.child(
                div()
                    .absolute()
                    .left(px((region.x0 as f32).max(0.0) + 8.0))
                    // Its own lane above the bar numbers, or "Loop" prints over a number.
                    .bottom(px(BAR_NUMBER_SIZE + 12.0))
                    .px(px(metrics(t).space[0]))
                    .bg(paint(ink.well.with_alpha(PLATE_ALPHA)))
                    .type_role(t, TypeRole::Label)
                    .text_color(paint(ink.loop_edge(region.looping)))
                    .child(caption),
            );
        }
    }
    for flag in &flags {
        el = el
            .child(
                div()
                    .absolute()
                    .left(px(flag.x))
                    .top(px(0.))
                    .w(px(ink.cue_width))
                    .h(px(height))
                    .bg(paint(ink.cue_guide())),
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
                    .bg(paint(ink.deck))
                    .text_color(paint(ink.on_deck))
                    .child(numeral(t, Numeral::Value, flag.label.clone())),
            );
    }
    el.on_scroll(cx.listener(move |this, ev: &gpui::ScrollEvent, _, cx| {
        let dy = f32::from(ev.delta.pixel_delta(px(16.)).y);
        let step = this.decks[index].zoom.wheel(dy);
        for deck in &mut this.decks {
            deck.zoom.set_zoom_step(step);
        }
        cx.notify();
    }))
}

// ---- The controls --------------------------------------------------------------------------

/// Deck `index`'s controls at `card` (window coordinates), from its [`DeckLayout`].
pub fn deck_card(shell: &Shell, index: usize, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let l = deck_controls_layout(Rect::sized(card.w, card.h), &m, is_mirrored(index));
    let snap = shell.engine.deck_snapshot(index).unwrap_or_default();
    let mode = shell.decks[index].pad_mode;
    let mut root = place(card)
        .text_color(paint(t.text_primary))
        .child(pitch(shell, index, card, l.pitch, cx))
        .child(transport(shell, index, &l, &snap, cx))
        .child(tabs(shell, index, &l, cx));
    root = match mode {
        PadMode::Cues if platter_state(index).on => {
            let (disc, row) = platter_layout(&l, &m);
            root.child(platter(shell, index, card, disc, &snap, cx))
                .child(options(shell, index, (row, l.mirrored), &snap, cx))
        }
        PadMode::Cues => root
            .children(hotcues(shell, index, &l, &snap, cx))
            .child(options(shell, index, (l.options, l.mirrored), &snap, cx)),
        PadMode::Loop => root
            .children(loop_pads(shell, index, &l, &snap, cx))
            .child(options(shell, index, (l.options, l.mirrored), &snap, cx)),
        PadMode::Stems => root.children(stems(shell, index, card, l.drawer, cx)),
        PadMode::Fx => root.child(crate::mixer_view::fx_drawer(shell, card, l.drawer, cx)),
    };
    load_on_drop(root, t, index, cx)
}

/// The pitch fader at `rect` (card coordinates): the deck colour on its cap, a centre notch, and
/// a tab stop for the keys.
fn pitch(shell: &Shell, index: usize, card: Rect, rect: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let deck = &shell.decks[index];
    let pos = fader_paint::pitch_pos(deck.pitch.fader_value(), deck.pitch.range());
    let abs = Rect::new(card.x + rect.x, card.y + rect.y, rect.w, rect.h);
    let target = Target::Pitch(index);
    let popup = shell.is_dragging(target).then(|| deck.pitch.readout());
    fader_paint::fader(
        t,
        rect,
        &fader_paint::FaderFace::pitch().for_density(&metrics(t)),
        pos,
        hue(t, index),
        target,
        abs,
        popup.as_deref(),
        cx,
    )
    .child(slider_focus(t, target, cx))
}

/// The face of a deck button: disabled with no track, else `on` lit in the deck hue.
fn deck_face(loaded: bool, on: bool) -> Face {
    match (loaded, on) {
        (false, _) => Face::Disabled,
        (true, true) => Face::On,
        (true, false) => Face::Rest,
    }
}

/// A transport press for `button` on deck `index`.
fn press(
    index: usize,
    button: TransportButton,
) -> impl Fn(&mut Shell, &mut Window, &mut Context<Shell>) {
    move |this, _, _| {
        this.decks[index].transport.press(button);
    }
}

/// The transport row: Play and Cue at the outside edge, Sync (with LOCK while synced) and Key lock
/// at the inner end. Play is the solid deck fill while playing.
fn transport(
    shell: &Shell,
    index: usize,
    l: &DeckLayout,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let h = px(m.control_height);
    let hue = hue(t, index);
    let loaded = snap.loaded;
    let play_face = match (loaded, snap.playing) {
        (false, _) => Face::Disabled,
        (true, true) => Face::Playing,
        (true, false) => Face::Rest,
    };
    let play = press_button(
        t,
        "Play",
        (play_face, hue),
        cx,
        press(index, TransportButton::Play),
    )
    .h(h)
    .flex_none()
    .min_w(px(m.control_height * 3.0));
    let cue = press_button(
        t,
        "Cue",
        (deck_face(loaded, false), hue),
        cx,
        press(index, TransportButton::Cue),
    )
    .h(h)
    .flex_none();
    let sync = face_button(
        t,
        "Sync",
        (deck_face(loaded, snap.sync), hue),
        cx,
        press(index, TransportButton::Sync),
    )
    .h(h)
    .flex_none();
    let mut sync_group = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(m.space[0]))
        .child(sync);
    if snap.sync && loaded {
        sync_group = sync_group.child(state_word(t, "LOCK", hue));
    }
    let key_lock = face_button(
        t,
        TransportButton::KeyLock.label(),
        (deck_face(loaded, snap.keylock), hue),
        cx,
        press(index, TransportButton::KeyLock),
    )
    .h(h)
    .flex_none();
    place(l.transport)
        .flex()
        .items_center()
        .gap(px(m.space[2]))
        .when(l.mirrored, |el| el.flex_row_reverse())
        .child(play)
        .child(cue)
        .child(div().flex_1())
        .child(sync_group)
        .child(key_lock)
}

/// The tabs row: Hot cues / Loop / Stems / FX at the outside edge; on the Stems tab, the button
/// that loads the deck's stems or turns them off at the inner end.
fn tabs(shell: &Shell, index: usize, l: &DeckLayout, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let mode = shell.decks[index].pad_mode;
    let labels: Vec<&str> = PadMode::ALL.iter().map(|p| p.label()).collect();
    let selected = PadMode::ALL.iter().position(|p| *p == mode);
    let group = segmented(
        t,
        &labels,
        (selected, hue(t, index)),
        cx,
        move |this: &mut Shell, i| {
            this.select_pad_mode(index, PadMode::ALL[i]);
        },
    );
    let mut row = place(l.tabs)
        .flex()
        .items_center()
        .gap(px(m.space[2]))
        .when(l.mirrored, |el| el.flex_row_reverse())
        .child(group)
        .child(div().flex_1());
    if mode == PadMode::Stems {
        let live = stems_live(shell, index);
        let label = if live { "Stems off" } else { "Load stems" };
        row = row.child(
            face_button(
                t,
                label,
                (Face::Rest, hue(t, index)),
                cx,
                move |this, _, _| this.toggle_deck_stems(index),
            )
            .h(px(m.control_height))
            .flex_none(),
        );
    }
    row
}

impl Shell {
    /// Opens `mode` in deck `index`'s drawer. The FX tab shows that deck's channel rack; the
    /// effects refresh while any deck has it open. Any tab closes the platter.
    pub(crate) fn select_pad_mode(&mut self, index: usize, mode: PadMode) {
        set_platter(index, |p| p.on = false);
        self.decks[index].pad_mode = mode;
        if mode == PadMode::Fx {
            self.mixer.fx.show_channel(index);
        }
        let fx_open = self.decks.iter().any(|d| d.pad_mode == PadMode::Fx);
        let page = if fx_open { Page::Fx } else { Page::Mixer };
        if self.mixer.page() != page {
            self.mixer.set_page(page);
        }
    }
}

/// Whether deck `index` is playing from the cue at `cue_frame`: within [`ACTIVE_WINDOW_S`] after it.
fn pad_playing(s: &DeckSnapshot, cue_frame: Option<i64>) -> bool {
    match cue_frame {
        Some(c) if s.playing && s.file_sample_rate > 0.0 => {
            let d = (s.position_frames - c) as f64 / s.file_sample_rate;
            (0.0..ACTIVE_WINDOW_S).contains(&d)
        }
        _ => false,
    }
}

/// The state a hot cue pad shows: disabled with no track, playing just after its cue, set, or
/// empty.
pub(crate) fn cue_pad_state(snap: &DeckSnapshot, set: bool, cue_frame: Option<i64>) -> PadState {
    match (snap.loaded, set) {
        (false, _) => PadState::Disabled,
        (true, true) if pad_playing(snap, cue_frame) => PadState::Playing,
        (true, true) => PadState::Set,
        (true, false) => PadState::Empty,
    }
}

/// The eight hot cue pads: the number, and "Cue" once set. A press plays from the cue (or sets
/// it); shift, alt or ctrl clears it.
fn hotcues(
    shell: &Shell,
    index: usize,
    l: &DeckLayout,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let m = metrics(t);
    let lit = shell.decks[index].hotcues.lit();
    let hue = hue(t, index);
    pad_grid(l.pads, m.space[1].round() as i32)
        .into_iter()
        .enumerate()
        .map(|(i, rect)| {
            let state = cue_pad_state(snap, lit[i], snap.hotcues[i]);
            let name = if lit[i] { "Cue" } else { "" };
            pad_button(
                t,
                rect,
                (&(i + 1).to_string(), name),
                (state, hue),
                &format!("cue-{index}-{i}"),
                cx,
                move |this, mods| {
                    let mods = Modifiers {
                        shift: mods.shift,
                        alt: mods.alt,
                        ctrl: mods.control,
                    };
                    this.decks[index].hotcues.click(i, mods);
                },
            )
        })
        .collect()
}

/// A loop length in beats as its pad names it: "1/4 beat", "4 beats".
pub(crate) fn loop_pad_name(i: usize) -> String {
    let unit = if SIZES[i] <= 1.0 { "beat" } else { "beats" };
    format!("{} {unit}", SIZE_NAMES[i])
}

/// The Loop tab's eight pads, one per loop size (1/4 to 32 beats); the size of the running loop is
/// lit. A press starts a loop of that size.
fn loop_pads(
    shell: &Shell,
    index: usize,
    l: &DeckLayout,
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let m = metrics(t);
    let frame: LoopFrame = shell.decks[index].loops.frame();
    let hue = hue(t, index);
    pad_grid(l.pads, m.space[1].round() as i32)
        .into_iter()
        .enumerate()
        .take(PAD_COUNT.min(SIZES.len()))
        .map(|(i, rect)| {
            let state = match (snap.loaded, frame.size_lit[i]) {
                (false, _) => PadState::Disabled,
                (true, true) => PadState::Playing,
                (true, false) => PadState::Set,
            };
            pad_button(
                t,
                rect,
                ("", &loop_pad_name(i)),
                (state, hue),
                &format!("loop-{index}-{i}"),
                cx,
                move |this, _| {
                    this.decks[index].loops.press_size(i);
                },
            )
        })
        .collect()
}

/// The row under the pads. Hot cues: Tempo leader, Snap to beat, Slip and the Platter switch.
/// Loop: the Loop switch (with the length while a loop runs), Halve, Double and Snap to beat.
fn options(
    shell: &Shell,
    index: usize,
    (rect, mirrored): (Rect, bool),
    snap: &DeckSnapshot,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let h = px(m.control_height);
    let hue = hue(t, index);
    let loaded = snap.loaded;
    let deck = &shell.decks[index];
    let toggle = |button: TransportButton, cx: &mut Context<Shell>| {
        face_button(
            t,
            button.label(),
            (deck_face(loaded, deck.transport.is_lit(button)), hue),
            cx,
            press(index, button),
        )
        .h(h)
        .flex_none()
    };
    let mut row = place(rect)
        .flex()
        .items_center()
        .gap(px(m.space[2]))
        .when(mirrored, |el| el.flex_row_reverse());
    if deck.pad_mode == PadMode::Loop {
        let frame = deck.loops.frame();
        let mut switch = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(m.space[0]))
            .child(
                face_button(
                    t,
                    "Loop",
                    (deck_face(loaded, frame.reloop_lit), hue),
                    cx,
                    move |this, _, _| {
                        this.decks[index].loops.toggle();
                    },
                )
                .h(h)
                .flex_none(),
            );
        if frame.reloop_lit && loaded {
            switch = switch.child(state_word(t, &frame.label(), hue));
        }
        let step = |label: &str, double: bool, cx: &mut Context<Shell>| {
            face_button(
                t,
                label,
                (deck_face(loaded, false), hue),
                cx,
                move |this, _, _| {
                    let loops = &this.decks[index].loops;
                    if double {
                        loops.double();
                    } else {
                        loops.halve();
                    }
                },
            )
            .h(h)
            .flex_none()
        };
        row = row
            .child(switch)
            .child(step("½", false, cx))
            .child(step("2×", true, cx))
            .child(toggle(TransportButton::Quantize, cx));
    } else {
        let on = platter_state(index).on;
        row = row
            .child(toggle(TransportButton::Master, cx))
            .child(toggle(TransportButton::Quantize, cx))
            .child(toggle(TransportButton::Slip, cx))
            .child(div().flex_1())
            .child(
                face_button(
                    t,
                    "Platter",
                    (if on { Face::On } else { Face::Rest }, hue),
                    cx,
                    move |_, _, _| set_platter(index, |p| p.on = !p.on),
                )
                .h(h)
                .flex_none(),
            );
    }
    row
}

/// Whether the deck plays from stems; also tells the stem model which track is loaded.
fn stems_live(shell: &Shell, index: usize) -> bool {
    let pads = &shell.decks[index].stems;
    if let Some(panel) = &shell.library {
        pads.observe_track(panel.loaded_track_id(index));
    }
    pads.stems()[0].available
}

/// The stems drawer at `area` (card coordinates): one row per stem with its name (the mute: lit
/// while the stem plays), solo, a volume slider and a filter slider with a centre detent. A deck
/// not playing from stems says how to get them.
fn stems(shell: &Shell, index: usize, card: Rect, area: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let m = metrics(t);
    let hue = hue(t, index);
    if !stems_live(shell, index) {
        let message = shell.stem_errors[index]
            .clone()
            .or_else(|| shell.library.as_ref().and_then(|p| p.stem_note(index)))
            .unwrap_or_else(|| "No stems for this track. Prepare stems in the library.".to_owned());
        return vec![place(area)
            .type_role(t, TypeRole::Body)
            .text_color(paint(t.text_secondary))
            .child(message)];
    }
    let pads = &shell.decks[index].stems;
    let row_h = m.control_height.round() as i32;
    let rows = stem_pad_layout(area, m.space[1].round() as i32, row_h);
    let abs = |r: Rect| Rect::new(card.x + r.x, card.y + r.y, r.w, r.h);
    let mut parts = Vec::new();
    for (stem, (state, row)) in pads.stems().iter().zip(rows).enumerate() {
        let soloed = pads.soloed() == Some(stem);
        let face = if state.muted { Face::Rest } else { Face::On };
        parts.push(
            place(row.mute).child(
                face_button(t, STEM_NAMES[stem], (face, hue), cx, move |this, _, _| {
                    this.decks[index].stems.toggle_mute(stem);
                })
                .size_full(),
            ),
        );
        let solo = if soloed { Face::On } else { Face::Rest };
        parts.push(
            place(row.solo).child(
                face_button(t, "Solo", (solo, hue), cx, move |this, _, _| {
                    this.decks[index].stems.solo(stem);
                })
                .size_full(),
            ),
        );
        let volume = Target::StemVolume(index, stem);
        parts.push(
            slider(
                t,
                row.volume,
                (f64::from(state.gain), 0.0, MAX_VOLUME),
                volume,
                abs(row.volume),
                cx,
            )
            .child(slider_focus(t, volume, cx)),
        );
        let filter = Target::StemFilter(index, stem);
        parts.push(
            slider(
                t,
                row.filter,
                (f64::from(state.filter), -1.0, 1.0),
                filter,
                abs(row.filter),
                cx,
            )
            .child(
                place(Rect::new(row.filter.w / 2, 2, 1, (row.filter.h - 4).max(1)))
                    .bg(paint(t.text_muted)),
            )
            .child(slider_focus(t, filter, cx)),
        );
    }
    parts
}

// ---- The platter ---------------------------------------------------------------------------

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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = cx;
        div()
            .px(px(6.))
            .py(px(2.))
            .rounded(px(2.))
            .bg(paint(self.fill))
            .text_color(paint(self.ink))
            .text_size(px(om_ui::theme::MIN_LABEL_SIZE))
            .child(self.text.clone())
    }
}

/// The platter in the place of the pads: the disc, the played arc on its ring, the turning
/// marker and the deck letter in the middle. Dragging the inner 85 % scrubs through `Seek`.
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
    let accent = hue(t, index);
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
    let label = place(Rect::new(
        (rect.w - label_d) / 2,
        (rect.h - label_d) / 2,
        label_d,
        label_d,
    ))
    .flex()
    .items_center()
    .justify_center()
    .child(deck_tag(t, &deck_letter(index).to_string(), accent));
    let loaded = snap.loaded;
    let length = snap.length_frames;
    let per_turn = SECONDS_PER_TURN * snap.file_sample_rate;
    let at = |x: gpui::Pixels, y: gpui::Pixels| (f32::from(x), f32::from(y));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(loaded: bool, playing: bool) -> DeckSnapshot {
        DeckSnapshot {
            loaded,
            playing,
            file_sample_rate: 48_000.0,
            position_frames: 48_000,
            ..DeckSnapshot::default()
        }
    }

    #[test]
    fn a_cue_pad_is_disabled_without_a_track_and_playing_just_after_its_cue() {
        assert_eq!(
            cue_pad_state(&snap(false, false), true, Some(0)),
            PadState::Disabled
        );
        assert_eq!(
            cue_pad_state(&snap(true, false), false, None),
            PadState::Empty
        );
        assert_eq!(
            cue_pad_state(&snap(true, false), true, Some(48_000)),
            PadState::Set
        );
        assert_eq!(
            cue_pad_state(&snap(true, true), true, Some(47_000)),
            PadState::Playing
        );
        assert_eq!(
            cue_pad_state(&snap(true, true), true, Some(0)),
            PadState::Set,
            "long after"
        );
    }

    #[test]
    fn loop_pads_name_their_length_in_beats() {
        assert_eq!(loop_pad_name(0), "1/4 beat");
        assert_eq!(loop_pad_name(2), "1 beat");
        assert_eq!(loop_pad_name(4), "4 beats");
        assert_eq!(SIZES.len(), PAD_COUNT, "one pad per loop size");
    }

    #[test]
    fn deck_buttons_are_disabled_without_a_track_and_lit_when_on() {
        assert_eq!(deck_face(false, true), Face::Disabled);
        assert_eq!(deck_face(true, true), Face::On);
        assert_eq!(deck_face(true, false), Face::Rest);
    }
}
