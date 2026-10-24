//! A window of rotary knobs in every state `knob_paint` draws, for checking the look by eye:
//! `cargo run --locked -p om-ui-gpui --example knob_gallery` (set `OPENMIXER_THEME` is not read
//! here; it uses the built-in dark theme, or the daylight one with `--light`).

use gpui::{
    canvas, div, point, prelude::*, px, size, App, Application, Bounds, Context, Pixels, Window,
    WindowBounds, WindowOptions,
};
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;
use om_ui_gpui::knob_paint::{
    angle_of, in_kill_zone, knob_geometry, paint_knob, KnobColours, KnobPaintState, KnobRole,
};
use om_ui_gpui::paint::paint;

/// One knob to draw: role, cell size, value over (min, detent, max), kill zone on, then the
/// hovered, dragging and disabled flags.
struct Spec {
    role: KnobRole,
    cell: (f32, f32),
    value: f64,
    range: (f64, Option<f64>, f64),
    kill: bool,
    flags: (bool, bool, bool),
}

const EQ: (f64, Option<f64>, f64) = (-48.0, Some(0.0), 6.0);
const TRIM: (f64, Option<f64>, f64) = (0.0, Some(1.0), 4.0);
const LINEAR: (f64, Option<f64>, f64) = (0.0, None, 1.0);

fn specs() -> Vec<Spec> {
    let s = |role, cell, value, range, kill, flags| Spec {
        role,
        cell,
        value,
        range,
        kill,
        flags,
    };
    let idle = (false, false, false);
    vec![
        s(KnobRole::Band, (58.0, 56.0), 0.0, EQ, true, idle),
        s(KnobRole::Band, (58.0, 56.0), 3.0, EQ, true, idle),
        s(KnobRole::Band, (58.0, 56.0), -20.0, EQ, true, idle),
        s(KnobRole::Band, (58.0, 56.0), -47.0, EQ, true, idle),
        s(
            KnobRole::Band,
            (58.0, 56.0),
            -20.0,
            EQ,
            true,
            (true, false, false),
        ),
        s(
            KnobRole::Band,
            (58.0, 56.0),
            -20.0,
            EQ,
            true,
            (true, true, false),
        ),
        s(
            KnobRole::Band,
            (58.0, 56.0),
            -20.0,
            EQ,
            true,
            (false, false, true),
        ),
        s(KnobRole::Wide, (116.0, 40.0), 1.0, TRIM, false, idle),
        s(KnobRole::Wide, (116.0, 40.0), 2.5, TRIM, false, idle),
        s(KnobRole::Cue, (60.0, 44.0), 0.0, LINEAR, false, idle),
        s(KnobRole::Cue, (60.0, 44.0), 0.65, LINEAR, false, idle),
        s(KnobRole::Cue, (60.0, 44.0), 1.0, LINEAR, false, idle),
        s(KnobRole::Fx, (44.0, 44.0), 0.3, LINEAR, false, idle),
        s(KnobRole::Sampler, (32.0, 32.0), 0.7, LINEAR, false, idle),
        s(KnobRole::Stem, (24.0, 24.0), 0.7, LINEAR, false, idle),
    ]
}

struct Gallery {
    tokens: ThemeTokens,
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let look = LookAndFeel::from_tokens(&self.tokens);
        let colours = KnobColours::new(&look, &self.tokens, self.tokens.accent_for(0));
        let mut row = div()
            .flex()
            .flex_wrap()
            .gap_4()
            .p_4()
            .size_full()
            .bg(paint(self.tokens.surface));
        for spec in specs() {
            let (min, detent, max) = spec.range;
            let geom = knob_geometry(spec.cell.0, spec.cell.1, 0.0, spec.role);
            let state = KnobPaintState {
                geom,
                angle: angle_of(spec.value, min, detent, max),
                detent_angle: detent.map(|d| angle_of(d, min, detent, max)),
                bipolar: detent.is_some(),
                killed: in_kill_zone(spec.value, min, detent, spec.kill),
                hovered: spec.flags.0,
                active: spec.flags.1,
                disabled: spec.flags.2,
            };
            row = row.child(
                div().w(px(spec.cell.0)).h(px(spec.cell.1)).child(
                    canvas(
                        |_, _, _| (),
                        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
                            paint_knob(window, bounds.origin, &state, &colours);
                        },
                    )
                    .size_full(),
                ),
            );
        }
        row
    }
}

fn main() {
    let light = std::env::args().any(|a| a == "--light");
    Application::new().run(move |cx: &mut App| {
        let tokens = if light {
            let (_, json) = om_ui::theme::BUILTIN_THEMES
                .iter()
                .find(|(file, _)| file.contains("daylight"))
                .expect("daylight theme");
            om_ui::theme::parse_theme(json).expect("parses").tokens
        } else {
            ThemeTokens::builtin_dark()
        };
        let bounds = Bounds {
            origin: point(px(100.0), px(100.0)),
            size: size(px(700.0), px(220.0)),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| cx.new(|_| Gallery { tokens }),
        )
        .expect("open window");
        cx.activate(true);
    });
}
