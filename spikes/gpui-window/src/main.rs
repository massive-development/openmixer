//! THROWAWAY SPIKE (#218): gpui-kit + cpal + midir in one binary.
//! One gpui-component page (Button) and one custom-painted knob (canvas).
//! Not production code; do not reuse.
use cpal::traits::HostTrait;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::*;

struct Page {
    info: String,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let angle = 0.7_f32;
        div()
            .size_full()
            .bg(rgb(0x101418))
            .text_color(rgb(0xffffff))
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(Button::new("ok").primary().label("gpui-component button"))
            .child(self.info.clone())
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // Knob body: a rounded quad with full radius = circle.
                        let d = f32::from(bounds.size.height.min(bounds.size.width));
                        let body = Bounds::new(bounds.origin, size(px(d), px(d)));
                        window.paint_quad(quad(
                            body,
                            px(d / 2.0),
                            rgb(0x2a3340),
                            px(2.0),
                            rgb(0x3fb6ff),
                            BorderStyle::default(),
                        ));
                        // Indicator dot at `angle`.
                        let r = d / 2.0 * 0.7;
                        let c = d / 2.0;
                        let dot = 6.0;
                        let p = point(
                            bounds.origin.x + px(c + r * angle.sin() - dot / 2.0),
                            bounds.origin.y + px(c - r * angle.cos() - dot / 2.0),
                        );
                        window.paint_quad(fill(
                            Bounds::new(p, size(px(dot), px(dot))),
                            rgb(0xffffff),
                        ));
                    },
                )
                .size(px(96.0)),
            )
    }
}

fn main() {
    let host = cpal::default_host();
    let out = host.output_devices().map(|d| d.count()).unwrap_or(0);
    let midi_ports = midir::MidiInput::new("spike")
        .map(|m| m.port_count())
        .unwrap_or(0);
    let info = format!("cpal output devices: {out}; midir input ports: {midi_ports}");
    println!("{info}");
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| Page { info })
        })
        .expect("failed to open window");
        cx.activate(true);
    });
}
