//! The controls gallery: every control part in every state, for one theme and one density. It is
//! the review surface of the faceplate controls (plan Phase 3) and of the phases after it. Each
//! state is forced, so hover, pressed and keyboard focus show without a pointer; a last row holds
//! live buttons. `examples/controls_gallery.rs` opens it in a window; the tests draw it headless in
//! every built-in theme and both densities.

use crate::controls::{face_button, knob_body, BarState, KnobBody};
use crate::fader_paint::{
    crossfader_face, fader_face, CrossfaderPlan, FaderFace, FaderLaw, FaderPlan,
};
use crate::font::TypeExt as _;
use crate::knob_paint::{angle_of, in_kill_zone, knob_geometry, KnobColours, KnobPaintState};
use crate::meter_paint::{meter_face, MeterFace};
use crate::paint::paint;
use crate::parts::{
    button_face, deck_tag, focused, pad_face, segmented_face, state_word, toggle_with_word,
    PadState,
};
use gpui::{div, prelude::*, px, Context, Div, Window};
use om_ui::geometry::Rect;
use om_ui::meter::column_width;
use om_ui::style::{Face, LookAndFeel};
use om_ui::theme::{parse_theme, Density, DensityMetrics, ThemeTokens, TypeRole, BUILTIN_THEMES};

/// A fader or meter column is this many pads tall (canvas "Fader": `pad * 2.4`).
const COLUMN_PADS: f32 = 2.4;
/// Width given to a knob's value beside it, px.
const VALUE_W: f32 = 52.0;
/// Width of a fader with its dB scale, px (the channel strip's 58).
const FADER_W: i32 = 58;
/// Length of the crossfader, px.
const CROSSFADER_W: i32 = 220;

/// The built-in themes as (display name, tokens), in the registry's order.
pub fn builtin_themes() -> Vec<(String, ThemeTokens)> {
    BUILTIN_THEMES
        .iter()
        .filter_map(|(_, json)| parse_theme(json).ok())
        .map(|p| (p.info.name, p.tokens))
        .collect()
}

/// The gallery for one theme at one density.
pub struct ControlsGallery {
    pub name: String,
    pub tokens: ThemeTokens,
    pub density: Density,
}

impl ControlsGallery {
    pub fn new(name: impl Into<String>, tokens: ThemeTokens, density: Density) -> Self {
        Self {
            name: name.into(),
            tokens,
            density,
        }
    }

    /// The pixel size the gallery is laid out for.
    pub fn size(density: Density) -> (f32, f32) {
        match density {
            Density::Roomy => (1180.0, 1060.0),
            Density::Compact => (1000.0, 860.0),
        }
    }
}

impl Render for ControlsGallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::controls::begin_frame();
        let t = self.tokens.clone();
        let m = *t.metrics(self.density);
        let title = format!("{}, {}", self.name, density_name(self.density));
        div()
            .debug_selector(|| "controls-gallery".into())
            .size_full()
            .flex()
            .flex_col()
            .gap(px(m.space[4]))
            .p(px(m.space[4]))
            .bg(paint(t.ground))
            .text_color(paint(t.text_primary))
            .font(crate::font::interface_font(&t))
            .child(heading(&t, &title))
            .child(section(&t, &m, "Buttons", buttons(&t, &m)))
            .child(section(&t, &m, "Toggles and state words", toggles(&t, &m)))
            .child(section(&t, &m, "Segmented and deck tags", segments(&t, &m)))
            .child(section(&t, &m, "Pads", pads(&t, &m)))
            .child(section(&t, &m, "Knobs", knobs(&t, &m)))
            .child(section(&t, &m, "Faders and meters", strips(&t, &m)))
            .child(section(&t, &m, "Live", live(&t, &m, cx)))
    }
}

fn density_name(d: Density) -> &'static str {
    match d {
        Density::Roomy => "Roomy",
        Density::Compact => "Compact",
    }
}

fn heading(t: &ThemeTokens, text: &str) -> Div {
    div()
        .type_role(t, TypeRole::Headline)
        .child(text.to_owned())
}

/// A captioned row of cells on the panel tone.
fn section(t: &ThemeTokens, m: &DensityMetrics, title: &str, cells: Vec<Div>) -> Div {
    let look = LookAndFeel::from_tokens(t);
    div()
        .flex()
        .flex_col()
        .gap(px(m.space[2]))
        .p(px(m.space[3]))
        .bg(paint(look.panel.fill))
        .border_1()
        .border_color(paint(look.panel.border))
        .rounded(px(m.panel_radius))
        .child(caption(t, title))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_end()
                .gap(px(m.space[4]))
                .children(cells),
        )
}

fn caption(t: &ThemeTokens, text: &str) -> Div {
    div()
        .type_role(t, TypeRole::Label)
        .text_color(paint(t.text_secondary))
        .whitespace_nowrap()
        .child(text.to_owned())
}

/// A control over its state name.
fn cell(t: &ThemeTokens, m: &DensityMetrics, name: &str, control: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(m.space[1]))
        .child(control)
        .child(caption(t, name))
}

fn buttons(t: &ThemeTokens, m: &DensityMetrics) -> Vec<Div> {
    let hue = t.accent_for(0);
    let mut cells: Vec<Div> = Face::ALL
        .iter()
        .map(|&f| {
            let label = if f == Face::Playing { "Play" } else { "Sync" };
            cell(t, m, f.name(), button_face(t, m, label, f, hue))
        })
        .collect();
    let ring = button_face(t, m, "Sync", Face::Rest, hue);
    cells.insert(3, cell(t, m, "Focus", focused(t, ring, m.control_radius)));
    cells
}

fn toggles(t: &ThemeTokens, m: &DensityMetrics) -> Vec<Div> {
    let (a, b) = (t.accent_for(0), t.accent_for(1));
    let loop_row = div()
        .flex()
        .gap(px(m.space[1]))
        .child(button_face(t, m, "Loop 4", Face::On, a))
        .child(button_face(t, m, "1/2", Face::Rest, a))
        .child(button_face(t, m, "2x", Face::Rest, a));
    vec![
        cell(
            t,
            m,
            "Off",
            toggle_with_word(t, m, "Sync", (Face::Rest, None), a),
        ),
        cell(
            t,
            m,
            "On (A)",
            toggle_with_word(t, m, "Sync", (Face::On, None), a),
        ),
        cell(
            t,
            m,
            "On (B)",
            toggle_with_word(t, m, "Sync", (Face::On, None), b),
        ),
        cell(
            t,
            m,
            "Sync locked",
            toggle_with_word(t, m, "Sync", (Face::On, Some("LOCK")), a),
        ),
        cell(t, m, "Loop active", loop_row),
        cell(t, m, "Clip", state_word(t, "CLIP", t.record_red())),
    ]
}

fn segments(t: &ThemeTokens, m: &DensityMetrics) -> Vec<Div> {
    let tabs = ["Hot cues", "Loop", "Stems", "FX"];
    let mut cells = vec![
        cell(
            t,
            m,
            "Deck A",
            segmented_face(t, m, &tabs, Some(0), t.accent_for(0)),
        ),
        cell(
            t,
            m,
            "Deck B",
            segmented_face(t, m, &tabs, Some(2), t.accent_for(1)),
        ),
        cell(
            t,
            m,
            "No deck",
            segmented_face(t, m, &["Full", "Controller"], Some(1), t.text_primary),
        ),
        cell(
            t,
            m,
            "Focus",
            focused(
                t,
                segmented_face(t, m, &["Full", "Controller"], Some(0), t.text_primary),
                m.control_radius,
            ),
        ),
    ];
    let tags = ["A", "B", "C", "D"]
        .iter()
        .enumerate()
        .fold(div().flex().gap(px(m.space[1])), |row, (i, l)| {
            row.child(deck_tag(t, l, t.accent_for(i as i32)))
        });
    cells.push(cell(t, m, "Deck tags", tags));
    cells
}

fn pads(t: &ThemeTokens, m: &DensityMetrics) -> Vec<Div> {
    let names = ["Intro", "Drop", "Break", "Outro"];
    let mut cells: Vec<Div> = PadState::ALL
        .iter()
        .zip(names)
        .enumerate()
        .map(|(i, (&state, name))| {
            let number = ["1", "2", "3", "4"][i];
            cell(
                t,
                m,
                state.name(),
                pad_face(t, m, (number, name), state, t.accent_for(0)),
            )
        })
        .collect();
    let deck_b = pad_face(t, m, ("5", "Vocal"), PadState::Set, t.accent_for(1));
    cells.push(cell(t, m, "Set (B)", deck_b));
    let ring = pad_face(t, m, ("6", "Build"), PadState::Set, t.accent_for(0));
    cells.push(cell(t, m, "Focus", focused(t, ring, m.panel_radius)));
    cells
}

/// One knob to draw: caption, value over (min, detent, max), value text, and the hovered,
/// dragging, focused and disabled flags.
struct KnobSpec {
    name: &'static str,
    value: f64,
    range: (f64, Option<f64>, f64),
    text: &'static str,
    flags: (bool, bool, bool, bool),
}

const EQ: (f64, Option<f64>, f64) = (-48.0, Some(0.0), 6.0);
const GAIN: (f64, Option<f64>, f64) = (0.0, None, 1.0);

fn knob_specs() -> [KnobSpec; 9] {
    let s = |name, value, range, text, flags| KnobSpec {
        name,
        value,
        range,
        text,
        flags,
    };
    let idle = (false, false, false, false);
    [
        s("Detent", 0.0, EQ, "0.0 dB", idle),
        s("Boost", 4.0, EQ, "+4.0 dB", idle),
        s("Cut", -24.0, EQ, "-24 dB", idle),
        s("Kill", -48.0, EQ, "-inf", idle),
        s("Hover", -12.0, EQ, "-12 dB", (true, false, false, false)),
        s("Dragging", -12.0, EQ, "-12 dB", (true, true, false, false)),
        s("Focus", 2.0, EQ, "+2.0 dB", (false, false, true, false)),
        s("Disabled", 0.0, EQ, "0.0 dB", (false, false, false, true)),
        s("Gain", 0.65, GAIN, "65%", idle),
    ]
}

fn knobs(t: &ThemeTokens, m: &DensityMetrics) -> Vec<Div> {
    let look = LookAndFeel::from_tokens(t);
    let colours = KnobColours::new(&look, t, t.accent_for(0));
    let label_h = (t.type_label * TypeRole::Label.line_height()).ceil();
    let (w, h) = (
        m.knob_size + 2.0 * crate::controls::KNOB_EDGE_PAD + VALUE_W,
        m.knob_size + label_h,
    );
    knob_specs()
        .into_iter()
        .map(|k| {
            let (min, detent, max) = k.range;
            let mut geom = knob_geometry(w, h, label_h, m.knob_size);
            geom.cx = m.knob_size / 2.0 + crate::controls::KNOB_EDGE_PAD;
            let (hovered, active, focused, disabled) = k.flags;
            let state = KnobPaintState {
                geom,
                angle: angle_of(k.value, min, detent, max),
                detent_angle: detent.map(|d| angle_of(d, min, detent, max)),
                bipolar: detent.is_some(),
                killed: in_kill_zone(k.value, min, detent, true),
                hovered,
                active,
                disabled,
                focused,
            };
            let body = KnobBody {
                geom,
                state,
                colours,
                label: k.name,
                label_below: true,
                value: Some(k.text),
            };
            let rect = Rect::sized(w.ceil() as i32, h.ceil() as i32);
            boxed(rect, knob_body(t, rect, body, None))
        })
        .collect()
}

/// `el` (placed absolutely at `rect`'s origin) in a box of `rect`'s size, so it takes room in a row.
fn boxed(rect: Rect, el: Div) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(rect.w as f32))
        .h(px(rect.h as f32))
        .child(el)
}

fn strips(t: &ThemeTokens, m: &DensityMetrics) -> Vec<Div> {
    let h = (m.pad_size * COLUMN_PADS).round() as i32;
    let rect = Rect::sized(FADER_W, h);
    let fader = |face: FaderFace, pos: f32, line| {
        let plan = FaderPlan::new(rect.w, rect.h, &face.for_density(m), pos);
        boxed(rect, fader_face(t, rect, &plan, line, None))
    };
    let unity = FaderLaw::Channel.pos_of(1.0);
    let quiet = FaderLaw::Channel.pos_at_db(-12.0);
    let master = FaderLaw::Master.pos_of(1.0);
    let meter_w = column_width(t.mixer_meter_width, true);
    let meter_rect = Rect::sized(meter_w, h);
    let meter = |level: f32, hold: f32, clipping: bool, face: MeterFace, readout: Option<&str>| {
        let bars = [BarState {
            level,
            hold,
            clipping,
        }; 2];
        let text = readout.map(str::to_owned);
        boxed(
            meter_rect,
            meter_face(t, meter_rect, (true, bars), face, text),
        )
    };
    let xf_rect = Rect::sized(CROSSFADER_W, (m.knob_size * 0.85).round() as i32);
    let xf_plan = CrossfaderPlan::sized(xf_rect.w, xf_rect.h, 0.42, m);
    let crossfader = div()
        .flex()
        .items_center()
        .gap(px(m.space[2]))
        .child(deck_tag(t, "A", t.accent_for(0)))
        .child(boxed(
            xf_rect,
            crossfader_face(t, xf_rect, &xf_plan, t.text_primary, None),
        ))
        .child(deck_tag(t, "B", t.accent_for(1)));
    vec![
        cell(
            t,
            m,
            "Channel A",
            fader(
                FaderFace::channel(FaderLaw::Channel),
                unity,
                t.accent_for(0),
            ),
        ),
        cell(
            t,
            m,
            "Channel B",
            fader(
                FaderFace::channel(FaderLaw::Channel),
                quiet,
                t.accent_for(1),
            ),
        ),
        cell(
            t,
            m,
            "Master",
            fader(FaderFace::master(FaderLaw::Master), master, t.text_primary),
        ),
        cell(
            t,
            m,
            "Pitch",
            fader(FaderFace::pitch(), 0.5, t.accent_for(0)),
        ),
        cell(
            t,
            m,
            "Healthy",
            meter(0.62, 0.7, false, MeterFace::channel(t), None),
        ),
        cell(
            t,
            m,
            "Hot",
            meter(0.97, 0.99, false, MeterFace::channel(t), None),
        ),
        cell(
            t,
            m,
            "Clip held",
            meter(0.4, 0.99, true, MeterFace::channel(t), None),
        ),
        cell(
            t,
            m,
            "Master",
            meter(0.9, 0.95, true, MeterFace::master(t), Some("-0.4")),
        ),
        cell(t, m, "Crossfader", crossfader),
    ]
}

/// Live buttons: hover, press and Tab focus them in a real window.
fn live(t: &ThemeTokens, m: &DensityMetrics, cx: &mut Context<ControlsGallery>) -> Vec<Div> {
    let hue = t.accent_for(0);
    let button = |label: &str, state: Face, cx: &mut Context<ControlsGallery>| {
        div().h(px(m.control_height)).flex().child(face_button(
            t,
            label,
            (state, hue),
            cx,
            |_, _, _| {},
        ))
    };
    vec![
        cell(t, m, "Rest", button("Cue", Face::Rest, cx)),
        cell(t, m, "On", button("Sync", Face::On, cx)),
        cell(t, m, "Playing", button("Play", Face::Playing, cx)),
        cell(t, m, "Disabled", button("Loop", Face::Disabled, cx)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{size, TestAppContext};

    #[test]
    fn the_three_built_in_themes_are_listed() {
        let names: Vec<String> = builtin_themes().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), 3, "{names:?}");
    }

    /// The gallery draws every part in every state, in each built-in theme and both densities,
    /// headless, without a panic, and lays out at the size it asks for.
    #[gpui::test]
    fn the_gallery_paints_in_every_theme_and_density(cx: &mut TestAppContext) {
        cx.update(|cx| crate::font::register(cx));
        for (name, tokens) in builtin_themes() {
            for density in [Density::Roomy, Density::Compact] {
                let (w, h) = ControlsGallery::size(density);
                let (view, vcx) = cx.add_window_view(|_, _| {
                    ControlsGallery::new(name.clone(), tokens.clone(), density)
                });
                vcx.simulate_resize(size(px(w), px(h)));
                vcx.run_until_parked();
                let bounds = vcx
                    .debug_bounds("controls-gallery")
                    .unwrap_or_else(|| panic!("{name} {density:?}: not drawn"));
                assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
                let quads = vcx.update(|window, _| window.painted_quads().len());
                assert!(quads > 50, "{name} {density:?}: only {quads} quads painted");
                drop(view);
            }
        }
    }
}
