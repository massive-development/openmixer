//! Shared control drawing and pointer wiring: push buttons, sliders, knobs and meters. Positions
//! are relative to the parent card (which is positioned); `abs` gives window coordinates for drags.
//! Buttons take the face states of `om_ui::style::Face` (drawn by `parts`): hover one tonal step
//! up, pressed into the well, the deck tint when on, and the 2px focus ring for keyboard focus.
//!
//! Knobs and faders print no value at rest. The value appears in a popup above the control while
//! it is dragged, and for knobs while the pointer hovers it (the hover is Shell state, so a hidden
//! popup never sits over a neighbouring button). A double-click puts a mixer control back to its
//! default.

use crate::app::Shell;
use crate::drag::{fraction, Target};
use crate::fader_paint::{self, FaderFace, FaderLaw};
use crate::font::{numeral, Numeral, TypeExt as _};
use crate::knob_paint::{
    angle_of, in_kill_zone, knob_geometry, paint_knob, sweep_fraction, value_of_fraction,
    KnobColours, KnobGeom, KnobPaintState, KnobRole, POPUP_LIFT,
};
use crate::meter_paint;
use crate::paint::{face, focus_layer, paint, place, skin};
use crate::parts::face_box;
use gpui::{
    canvas, div, prelude::*, px, App, Bounds, ClickEvent, Context, Div, FocusHandle, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseUpEvent, Pixels, ScrollEvent, SharedString, Stateful, Window,
};
use om_ui::geometry::Rect;
use om_ui::style::{Face, LookAndFeel};
use om_ui::theme::{Colour, Density, DensityMetrics, ThemeTokens, TypeRole};

/// A knob cell this tall or taller puts its label under the knob; shorter ones put it beside.
const LABEL_BELOW_MIN_H: i32 = 48;
/// Gap between the left edge of a cell and a knob that has its label beside it, px. Unsourced.
pub const KNOB_EDGE_PAD: f32 = 4.0;
/// Space between a knob's track and a label beside it, px. Unsourced.
const LABEL_GAP: f32 = 6.0;
/// Pixels one wheel line is worth when the wheel reports pixels.
const WHEEL_LINE_PX: f32 = 16.0;

/// Share of a knob's sweep one wheel notch or arrow key moves (spec section 3: 1/100).
const STEP_FRACTION: f64 = 0.01;
/// The same with shift held: 1/1000.
const FINE_STEP_FRACTION: f64 = 0.001;
/// Steps a Page Up or Page Down key moves.
const PAGE_STEPS: f64 = 10.0;

/// The value after `notches` wheel notches over a knob (positive: more). One notch is 1/100 of
/// the sweep, 1/1000 with shift. Like a drag, it moves along the sweep, not the raw range.
pub fn wheel_step(
    value: f64,
    notches: f64,
    shift: bool,
    min: f64,
    detent: Option<f64>,
    max: f64,
) -> f64 {
    let step = if shift {
        FINE_STEP_FRACTION
    } else {
        STEP_FRACTION
    };
    nudge(value, notches * step, min, detent, max)
}

/// `value` moved `delta` of the sweep.
fn nudge(value: f64, delta: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    let fraction = sweep_fraction(value, min, detent, max) + delta;
    value_of_fraction(fraction, min, detent, max)
}

/// The value a key press gives a focused knob, or None for a key the knob does not use. Up and
/// Right are one step more, Down and Left one less, Page Up and Page Down ten steps; shift makes
/// a step finer. Home goes to `reset` (the default) and End to the maximum.
pub fn key_step(
    key: &str,
    shift: bool,
    (value, reset): (f64, f64),
    (min, detent, max): (f64, Option<f64>, f64),
) -> Option<f64> {
    let step = if shift {
        FINE_STEP_FRACTION
    } else {
        STEP_FRACTION
    };
    let by = |steps: f64| Some(nudge(value, steps * step, min, detent, max));
    match key {
        "up" | "right" => by(1.0),
        "down" | "left" => by(-1.0),
        "pageup" => by(PAGE_STEPS),
        "pagedown" => by(-PAGE_STEPS),
        "home" => Some(reset),
        "end" => Some(max),
        _ => None,
    }
}

/// True when a press resets a knob: a double-click, or a click with ctrl or cmd held.
pub fn is_reset_click(click_count: usize, ctrl_or_cmd: bool) -> bool {
    click_count >= 2 || ctrl_or_cmd
}

/// The metric set the window lays out with. Density is not a preference yet (plan D8), so the
/// window is Roomy; the gallery draws both.
pub fn metrics(t: &ThemeTokens) -> DensityMetrics {
    *t.metrics(Density::default())
}

/// A labelled push button at `rect`. `accent` lights it ("on") when `lit`.
pub fn button(
    t: &ThemeTokens,
    rect: Rect,
    label: &str,
    lit: bool,
    accent: Colour,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    place(rect).child(
        push_button(t, label, lit.then_some(accent), cx, move |this, _, _| {
            on_press(this)
        })
        .w_full()
        .h_full(),
    )
}

thread_local! {
    /// How many buttons of each label this frame has made so far (see [`next_button_id`]).
    static BUTTON_IDS: std::cell::RefCell<std::collections::HashMap<String, u32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Forgets the button ids of the last frame. Called at the start of each frame's render.
pub(crate) fn begin_frame() {
    BUTTON_IDS.with(|ids| ids.borrow_mut().clear());
}

/// An element id for a button labelled `label`: the label and how many same-labelled buttons came
/// before it in this frame. Same-labelled buttons (every watch folder row has a "Remove") would
/// otherwise share one focus handle; the order of a page is stable from frame to frame, so each
/// button keeps its id.
fn next_button_id(label: &str) -> String {
    BUTTON_IDS.with(|ids| {
        let mut ids = ids.borrow_mut();
        let seen = ids.entry(label.to_owned()).or_insert(0);
        let id = format!("button-{label}-{seen}");
        *seen += 1;
        id
    })
}

/// A push button that runs `on_press` on the view. `lit` is the deck hue it shows "on" in (a
/// toggle that is on), or `None` for a button at rest. See [`face_button`].
pub fn push_button<V: 'static>(
    t: &ThemeTokens,
    label: &str,
    lit: Option<Colour>,
    cx: &mut Context<V>,
    on_press: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Stateful<Div> {
    let (state, hue) = match lit {
        Some(hue) => (Face::On, hue),
        None => (Face::Rest, t.text_primary),
    };
    face_button(t, label, (state, hue), cx, on_press)
}

thread_local! {
    /// The focus handle of each focusable control, by element id, so a control keeps its keyboard
    /// focus from frame to frame and its focus ring can ask whether it has it.
    static FOCUS: std::cell::RefCell<std::collections::HashMap<String, FocusHandle>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The focus handle of the control with element id `id`: a tab stop, made on first use.
pub(crate) fn focus_handle(id: &str, cx: &App) -> FocusHandle {
    FOCUS.with(|handles| {
        handles
            .borrow_mut()
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    })
}

/// A button in `state` (`Rest`, `On`, `Playing` or `Disabled`; `hue` lights the last three) that
/// runs `on_press` on a click, or on Enter or Space while it has keyboard focus. The pointer takes
/// it one tonal step up and a press sinks it into the well; keyboard focus draws the 2px ring.
/// A disabled button takes no input and no focus. Its size is the caller's (a rect or a flex row).
pub fn face_button<V: 'static>(
    t: &ThemeTokens,
    label: &str,
    (state, hue): (Face, Colour),
    cx: &mut Context<V>,
    on_press: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Stateful<Div> {
    let look = LookAndFeel::from_tokens(t);
    let style = look.face(state, hue);
    let id = next_button_id(label);
    let el = face_box(t, &style, label.to_owned())
        .relative()
        .px(px(metrics(t).space[3]))
        .id(SharedString::from(id.clone()))
        .role(gpui::accesskit::Role::Button)
        .aria_label(label.to_owned());
    if state == Face::Disabled {
        return el;
    }
    let handle = focus_handle(&id, cx);
    let (hover, pressed) = (
        look.face(state.hovered(), hue),
        look.face(Face::Pressed, hue),
    );
    el.track_focus(&handle)
        .child(focus_layer(look.focus, style.radius, Some(handle)))
        .hover(move |s| face(s, &hover))
        .active(move |s| face(s, &pressed))
        // Keyboard focus only: a click does not take focus from the window's keys.
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            on_press(this, window, cx);
            cx.notify();
        }))
}

/// A transparent layer over its parent that shows `text` in a tooltip bubble while the pointer
/// rests on it. It takes no clicks of its own, so the parent's handlers still run.
pub(crate) fn tip_overlay(
    t: &ThemeTokens,
    id: impl Into<String>,
    text: impl Into<String>,
) -> gpui::Stateful<Div> {
    let style = LookAndFeel::from_tokens(t).tooltip;
    let text = text.into();
    div()
        .id(gpui::ElementId::Name(id.into().into()))
        .absolute()
        .size_full()
        .tooltip(move |_, cx| {
            cx.new(|_| crate::deck_view::Tip {
                text: text.clone(),
                fill: style.fill,
                ink: style.text,
            })
            .into()
        })
}

/// Makes `el` start a drag on `target` when pressed; a double-click resets the control to its
/// default instead. `abs` is the control's window rectangle.
pub fn draggable(el: Div, target: Target, abs: Rect, cx: &mut Context<Shell>) -> Div {
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            if ev.click_count >= 2 && this.reset_target(target) {
                this.end_drag();
            } else {
                this.begin_drag(target, abs, ev.position);
            }
            cx.notify();
        }),
    )
}

/// A value popup centred above a control `width` px wide, drawn at `top` (negative: above): the
/// control's name, when given, then its value in mono. It has no pointer handling, so it never
/// takes a press from what lies under it.
fn popup(t: &ThemeTokens, width: i32, top: f32, name: Option<&str>, value: &str) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let mut chip = skin(div().px(px(6.)).py(px(1.)), &look.tooltip)
        .flex()
        .items_center()
        .gap(px(4.))
        .type_role(t, TypeRole::Label)
        .whitespace_nowrap();
    if let Some(name) = name {
        chip = chip.child(name.to_owned());
    }
    let chip = chip.child(numeral(t, Numeral::Value, value.to_owned()));
    div()
        .absolute()
        .left(px(0.))
        .top(px(top))
        .w(px(width as f32))
        .flex()
        .justify_center()
        .child(chip)
}

/// Vertical or horizontal slider track with its thumb at the value's fraction. Drawn in `rect`
/// (parent coordinates) and made draggable with `abs`.
pub fn slider(
    t: &ThemeTokens,
    rect: Rect,
    range: (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    slider_with_popup(t, rect, range, target, abs, None, cx)
}

/// [`slider`] with a value popup: `popup_text` is shown above the track while `Some`, which the
/// caller passes while the slider is being dragged.
pub fn slider_with_popup(
    t: &ThemeTokens,
    rect: Rect,
    (value, min, max): (f64, f64, f64),
    target: Target,
    abs: Rect,
    popup_text: Option<&str>,
    cx: &mut Context<Shell>,
) -> Div {
    let vertical = rect.h >= rect.w;
    // A channel fader's cap line is its deck's colour; the others are ivory.
    let accent = match target {
        Target::Channel(ch) => t.accent_for(ch as i32),
        _ => t.text_primary,
    };
    if target == Target::Crossfader && !vertical {
        let pos = fraction(value, min, max) as f32;
        return fader_paint::crossfader(t, rect, pos, accent, target, abs, popup_text, cx);
    }
    if vertical {
        // The stored value stays linear gain; the law only places the cap (and drag.rs maps the
        // pointer back through the same law). Other vertical sliders keep the linear travel.
        let (law, face): (_, fn(FaderLaw) -> FaderFace) = match target {
            Target::Master => (FaderLaw::Master, FaderFace::master),
            Target::Channel(_) => (FaderLaw::Channel, FaderFace::channel),
            _ => (
                FaderLaw::Linear {
                    max_gain: max as f32,
                },
                FaderFace::channel,
            ),
        };
        let face = face(law);
        let pos = match law {
            FaderLaw::Linear { .. } => fraction(value, min, max) as f32,
            tapered => tapered.pos_of(value as f32),
        };
        return fader_paint::fader(t, rect, &face, pos, accent, target, abs, popup_text, cx);
    }
    // Other horizontal sliders (stems, phones) keep the plain track and thumb.
    let look = LookAndFeel::from_tokens(t);
    let frac = fraction(value, min, max) as f32;
    let thumb_px = look.fader.thumb_size.round() as i32;
    let travel = (rect.w - thumb_px).max(0) as f32;
    let thumb = place(Rect::new((frac * travel) as i32, 0, thumb_px, rect.h));
    let track = skin(place(rect), &look.fader.track);
    let mut track = track.child(skin(thumb, &look.fader.thumb));
    if let Some(text) = popup_text {
        track = track.child(popup(t, rect.w, -POPUP_LIFT, None, text));
    }
    draggable(track, target, abs, cx)
}

/// What a knob prints and where its detent is.
#[derive(Debug, Clone, Copy)]
pub struct KnobFace<'a> {
    /// Printed in the middle of the knob; empty for a knob whose label sits elsewhere.
    pub label: &'a str,
    /// The value text for the popup.
    pub readout: &'a str,
    /// The value the knob rests on (its detent), marked on the body.
    pub detent: Option<f64>,
    /// The popup is shown: the knob is being dragged or hovered (see `Shell::shows_value`).
    pub active: bool,
}

/// A compact knob for cards that draw their own (the deck's stem filter): the label on top and the
/// value at the bottom of the body, always visible, because those knobs are too small for a popup.
/// A bipolar range is marked at zero.
pub fn knob(
    t: &ThemeTokens,
    rect: Rect,
    (label, readout): (&str, &str),
    (value, min, max): (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let (inset_s, inset_l) = (look.inset_small, look.inset_large);
    // A narrow knob drops the unit so the readout stays on one line.
    let readout = if rect.w < 48 {
        readout.strip_suffix(" dB").unwrap_or(readout)
    } else {
        readout
    };
    // A very narrow knob has room for the value only.
    let label = if rect.w < 36 { "" } else { label };
    let frac = fraction(value, min, max) as f32;
    let fill_h = ((rect.h as f32 - inset_l) * frac) as i32;
    let mut body = skin(place(rect), &look.knob.body)
        .overflow_hidden()
        .text_color(paint(look.knob.readout.text))
        .child(
            place(Rect::new(
                0,
                rect.h - fill_h - inset_s as i32,
                rect.w,
                fill_h,
            ))
            .bg(paint(look.knob.fill.with_multiplied_alpha(0.45)))
            .rounded(px(look.bar_radius)),
        );
    if min < 0.0 && max > 0.0 {
        let zero = fraction(0.0, min, max) as f32;
        let y = ((rect.h as f32 - inset_l) * (1.0 - zero)) as i32 + inset_s as i32;
        body = body.child(
            place(Rect::new(0, y, rect.w, 1))
                .bg(paint(look.knob.readout.text.with_multiplied_alpha(0.5))),
        );
    }
    let body = body
        .child(
            div()
                .absolute()
                .left(px(inset_l))
                .top(px(inset_s))
                .child(label.to_owned()),
        )
        .child(
            div()
                .absolute()
                .left(px(inset_l))
                .bottom(px(inset_s))
                .child(readout.to_owned()),
        );
    draggable(body, target, abs, cx)
}

/// How a rotary knob is drawn beyond what [`KnobFace`] says. The default is a band (EQ) knob in
/// the first channel accent, idle and enabled, at the window's density.
#[derive(Debug, Clone, Copy, Default)]
pub struct KnobLook {
    /// Where the label goes; see [`KnobRole`].
    pub role: KnobRole,
    /// The value arc colour (the strip accent); the theme's first accent when None.
    pub accent: Option<Colour>,
    /// Being dragged: the arc lifts toward the pointer colour.
    pub dragging: bool,
    /// Nothing to control (an empty sampler pad): faded, no input, no popup.
    pub disabled: bool,
    /// The knob's diameter comes from this density's metric set (34 Roomy, 26 Compact).
    pub density: Density,
    /// The value in mono beside the knob at rest (DESIGN.md Components, "Knobs"), instead of only
    /// in the popup. Needs room to the knob's right.
    pub value_beside: bool,
}

/// The caption line under a knob whose label sits below it, px.
fn label_line_height(t: &ThemeTokens) -> f32 {
    (t.type_label * TypeRole::Label.line_height()).ceil()
}

/// A rotary knob with the default look. See [`knob_face_with`].
pub fn knob_face(
    t: &ThemeTokens,
    rect: Rect,
    face: KnobFace<'_>,
    range: (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    knob_face_with(t, rect, face, range, (target, abs), KnobLook::default(), cx)
}

/// Where a knob and its label sit in a rect `w` x `h`: the geometry, and whether the label goes
/// below (tall cells, the EQ) or beside (short ones).
fn knob_layout(
    t: &ThemeTokens,
    (w, h): (i32, i32),
    label: &str,
    knob_look: &KnobLook,
) -> (KnobGeom, bool) {
    let show_label = !label.is_empty() && knob_look.role.shows_label();
    let label_below = show_label && h >= LABEL_BELOW_MIN_H;
    let label_h = if label_below {
        label_line_height(t)
    } else {
        0.0
    };
    let size = t.metrics(knob_look.density).knob_size;
    let mut geom = knob_geometry(w as f32, h as f32, label_h, size);
    if (show_label && !label_below) || knob_look.value_beside {
        // Room for the label or value: the knob moves to the left edge instead of the middle.
        let (_, _, side) = geom.bbox();
        geom.cx = side / 2.0 + KNOB_EDGE_PAD;
    }
    (geom, label_below)
}

/// What [`knob_body`] paints and prints.
#[derive(Debug, Clone, Copy)]
pub struct KnobBody<'a> {
    pub geom: KnobGeom,
    pub state: KnobPaintState,
    pub colours: KnobColours,
    pub label: &'a str,
    pub label_below: bool,
    /// The value printed beside the knob, when shown at rest.
    pub value: Option<&'a str>,
}

/// The drawn part of a knob in `rect` (no input): the painted knob, its label below or beside it,
/// and the value in mono beside it. With `focus`, the ring follows that handle's keyboard focus;
/// without, `body.state.focused` decides (the gallery).
pub fn knob_body(
    t: &ThemeTokens,
    rect: Rect,
    body: KnobBody<'_>,
    focus: Option<FocusHandle>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let (state, colours, geom) = (body.state, body.colours, body.geom);
    let painter = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let mut state = state;
            if let Some(h) = &focus {
                state.focused = h.is_focused(window) && window.last_input_was_keyboard();
            }
            paint_knob(window, bounds.origin, &state, &colours);
        },
    )
    .absolute()
    .size_full();
    let fade = if state.disabled { 0.4 } else { 1.0 };
    let mut outer = place(rect)
        .type_role(t, TypeRole::Label)
        .text_color(paint(look.knob.label.text.with_multiplied_alpha(fade)))
        .child(painter);
    let (_, _, side) = geom.bbox();
    let beside = geom.cx + side / 2.0 + LABEL_GAP;
    let column = |left: f32, top: f32, w: f32, h: f32| {
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(w.max(0.)))
            .h(px(h))
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
    };
    let (w, h) = (rect.w as f32, rect.h as f32);
    if !body.label.is_empty() {
        let label = if body.label_below {
            let line = label_line_height(t);
            if body.value.is_some() {
                // The value sits beside: the label starts under the knob's left edge, unclipped.
                let left = (geom.cx - side / 2.0).max(0.0);
                column(left, h - line, w - left, line)
            } else {
                column(0.0, h - line, w, line).justify_center()
            }
        } else {
            column(beside, 0.0, w - beside, h)
        };
        outer = outer.child(label.child(body.label.to_owned()));
    }
    if let Some(value) = body
        .value
        .filter(|_| body.label_below || body.label.is_empty())
    {
        let top = geom.cy - h / 2.0;
        outer = outer.child(
            column(beside, top, w - beside, h)
                .text_color(paint(look.knob.readout.text.with_multiplied_alpha(fade)))
                .child(numeral(t, Numeral::Value, value.to_owned())),
        );
    }
    outer
}

/// A rotary knob in `rect`: the 2px arc with the value from the detent (or minimum), the raised cap
/// with its pointer, the label below or beside it, the value beside it when `value_beside` is set,
/// and, while `face.active`, the value in a popup. `at` is the target and the rect's window
/// coordinates.
///
/// Only the disc of the knob takes the pointer: the wide trim bar is not one big button. It
/// responds to a press (drag; double-click or ctrl/cmd-click reset), shift (fine), the wheel, and
/// the keys of [`key_step`] when focused, which also draws its focus ring. Hovering tells the
/// Shell, which turns `active` on.
pub fn knob_face_with(
    t: &ThemeTokens,
    rect: Rect,
    face: KnobFace<'_>,
    (value, min, max): (f64, f64, f64),
    (target, abs): (Target, Rect),
    knob_look: KnobLook,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let (geom, label_below) = knob_layout(t, (rect.w, rect.h), face.label, &knob_look);
    let detent = face.detent.filter(|d| *d > min && *d < max);
    let state = KnobPaintState {
        geom,
        angle: angle_of(value, min, detent, max),
        detent_angle: detent.map(|d| angle_of(d, min, detent, max)),
        bipolar: detent.is_some(),
        killed: in_kill_zone(value, min, detent, matches!(target, Target::Eq(..))),
        hovered: face.active && !knob_look.disabled,
        active: knob_look.dragging,
        disabled: knob_look.disabled,
        focused: false,
    };
    let colours = KnobColours::new(&look, t, knob_look.accent.unwrap_or(look.knob.fill));
    let id = format!("knob-{target:?}");
    let handle = (!knob_look.disabled).then(|| focus_handle(&id, cx));
    let label = if knob_look.role.shows_label() {
        face.label
    } else {
        ""
    };
    let body = KnobBody {
        geom,
        state,
        colours,
        label,
        label_below,
        value: knob_look.value_beside.then_some(face.readout),
    };
    let mut outer = knob_body(t, rect, body, handle.clone());

    // The pressable part is the knob's bounding square, and a press outside the disc itself is
    // ignored. The popup is the wrapper's other child, so it exists (and takes the pointer) only
    // while shown, and then it takes nothing.
    let (bx, by, side) = geom.bbox();
    let hit_rect = Rect::new(
        bx.floor() as i32,
        by.floor() as i32,
        side.ceil() as i32,
        side.ceil() as i32,
    );
    let hit = place(hit_rect).id(gpui::ElementId::Name(id.into()));
    let hit = match handle {
        None => hit,
        Some(handle) => knob_input(hit.track_focus(&handle), target, abs, geom, cx),
    };
    outer = outer.child(hit);
    if face.active && !knob_look.disabled {
        let name = (!face.label.is_empty()).then_some(face.label);
        let top = geom.popup_top(rect.y as f32);
        outer = outer.child(popup(t, rect.w, top, name, face.readout));
    }
    outer
}

/// The pointer, wheel and key handling of a knob's hit square.
fn knob_input(
    hit: Stateful<Div>,
    target: Target,
    abs: Rect,
    geom: KnobGeom,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    hit.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let local_x = f32::from(ev.position.x) - abs.x as f32;
            let local_y = f32::from(ev.position.y) - abs.y as f32;
            if !geom.hits(local_x, local_y) {
                return;
            }
            let reset = is_reset_click(
                ev.click_count,
                ev.modifiers.control || ev.modifiers.platform,
            );
            if reset && this.reset_target(target) {
                this.end_drag();
            } else {
                this.begin_drag_fine(target, abs, ev.position, ev.modifiers.shift);
            }
            cx.notify();
        }),
    )
    .on_scroll(cx.listener(move |this, ev: &ScrollEvent, _, cx| {
        let lines = f64::from(f32::from(ev.delta.pixel_delta(px(WHEEL_LINE_PX)).y))
            / f64::from(WHEEL_LINE_PX);
        if lines != 0.0 {
            this.knob_wheel(target, lines, ev.modifiers.shift);
            cx.stop_propagation();
            cx.notify();
        }
    }))
    .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
        let keys = &ev.keystroke;
        if keys.modifiers.control || keys.modifiers.platform || keys.modifiers.alt {
            return;
        }
        if this.knob_key(target, keys.key.as_str(), keys.modifiers.shift) {
            cx.stop_propagation();
            cx.notify();
        }
    }))
    .on_hover(cx.listener(move |this, over: &bool, _, cx| {
        if *over {
            this.hovered = Some(target);
        } else if this.hovered == Some(target) {
            this.hovered = None;
        }
        cx.notify();
    }))
}

/// One side of a stereo meter, as the view model reports it.
#[derive(Debug, Clone, Copy, Default)]
pub struct BarState {
    /// Bar height, `0..=1`.
    pub level: f32,
    /// Peak-hold tick height, `0..=1`.
    pub hold: f32,
    /// The latched clip lamp.
    pub clipping: bool,
}

/// One stereo meter column: an optional dB scale, then two segmented bars coloured by dB zone (the
/// theme's `meter_mid_db` / `meter_high_db`), each with a peak-hold segment and a latching clip lamp
/// above it. See `meter_paint`.
/// A click anywhere on the column runs `on_click` (it clears the lamps).
pub fn stereo_meter(
    t: &ThemeTokens,
    rect: Rect,
    labels: bool,
    bars: [BarState; 2],
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell) + 'static,
) -> Div {
    meter_paint::segmented_meter(t, rect, labels, bars, cx, on_click)
}

/// A text caption at `rect` (parent coordinates), in the label tier.
pub fn caption(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    let style = LookAndFeel::from_tokens(t).panel_caption;
    place(rect)
        .type_role(t, TypeRole::Label)
        .text_color(paint(style.text))
        .child(text.to_owned())
}

/// A number caption at `rect` (parent coordinates): [`caption`] for a value, in mono.
pub fn value_caption(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    let style = LookAndFeel::from_tokens(t).panel_caption;
    place(rect)
        .text_color(paint(style.text))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(numeral(t, Numeral::Value, text.to_owned()))
}

/// Text centred in `rect`, for a label that rides on another control (the end marks of the
/// crossfader, the names inside the phones sliders). It has no pointer handling, so the control
/// under it still takes the press.
pub fn mark(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    place(rect)
        .flex()
        .items_center()
        .justify_center()
        .type_role(t, TypeRole::Label)
        .text_color(paint(t.text_secondary))
        .child(text.to_owned())
}

/// [`mark`] for a value (a meter's peak readout), in mono.
pub fn value_mark(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    place(rect)
        .flex()
        .items_center()
        .justify_center()
        .text_color(paint(t.text_secondary))
        .child(numeral(t, Numeral::Value, text.to_owned()))
}

/// Releases the drag when the pointer goes up anywhere in the window.
pub fn release_on(el: Div, cx: &mut Context<Shell>) -> Div {
    el.on_mouse_up(
        MouseButton::Left,
        cx.listener(|this, _: &MouseUpEvent, _, cx| {
            this.end_drag();
            cx.notify();
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knob_paint::{angle_of, sweep_fraction};

    const EQ: (f64, Option<f64>, f64) = (-48.0, Some(0.0), 6.0);

    fn frac(v: f64) -> f64 {
        sweep_fraction(v, EQ.0, EQ.1, EQ.2)
    }

    #[test]
    fn wheel_notch_is_one_hundredth_of_the_sweep() {
        let (min, d, max) = EQ;
        let up = wheel_step(0.0, 1.0, false, min, d, max);
        assert!((frac(up) - 0.51).abs() < 1e-9);
        let down = wheel_step(0.0, -3.0, false, min, d, max);
        assert!((frac(down) - 0.47).abs() < 1e-9);
        let fine = wheel_step(0.0, 1.0, true, min, d, max);
        assert!((frac(fine) - 0.501).abs() < 1e-9);
        assert!(angle_of(up, min, d, max) > 0.0);
        assert_eq!(wheel_step(6.0, 5.0, false, min, d, max), 6.0, "clamps");
    }

    #[test]
    fn keys_step_reset_and_jump() {
        let (min, d, max) = EQ;
        let key = |k: &str, shift: bool| key_step(k, shift, (0.0, 0.0), (min, d, max));
        let step = |v: f64| (frac(v) - 0.5).abs();
        assert!((step(key("up", false).unwrap()) - 0.01).abs() < 1e-9);
        assert!(key("right", false).unwrap() > 0.0);
        assert!(key("down", false).unwrap() < 0.0);
        assert!(key("left", false).unwrap() < 0.0);
        assert!((step(key("up", true).unwrap()) - 0.001).abs() < 1e-9);
        assert!((step(key("pageup", false).unwrap()) - 0.1).abs() < 1e-9);
        assert!(key("pagedown", false).unwrap() < 0.0);
        assert_eq!(key("end", false), Some(6.0));
        assert_eq!(
            key_step("home", false, (3.0, 1.0), (min, d, max)),
            Some(1.0)
        );
        assert_eq!(key("x", false), None);
        assert_eq!(key("enter", false), None);
    }

    #[test]
    fn ctrl_click_resets() {
        assert!(is_reset_click(2, false));
        assert!(is_reset_click(1, true));
        assert!(is_reset_click(3, true));
        assert!(!is_reset_click(1, false));
    }
}
