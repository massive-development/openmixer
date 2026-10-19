//! THROWAWAY SPIKE (Task 6): GPUI window with a custom-painted scrolling waveform
//! from synthetic data (2000 bars, sine mix) and an fps counter overlay.
//! Not production code; do not reuse.
use gpui::{
    canvas, div, fill, point, prelude::*, px, rgb, size, App, Application, Bounds, Context,
    Window, WindowBounds, WindowOptions,
};
use std::time::Instant;

const BARS: usize = 2000;

struct Waveform {
    start: Instant,
    last: Instant,
    frames: u32,
    fps: f32,
}

impl Render for Waveform {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // fps counter: recompute once per second.
        self.frames += 1;
        let dt = self.last.elapsed().as_secs_f32();
        if dt >= 1.0 {
            self.fps = self.frames as f32 / dt;
            println!("fps {:.1}", self.fps); // spike: log so headless runs are checkable
            self.frames = 0;
            self.last = Instant::now();
        }
        // Ask for another frame immediately so we paint continuously.
        window.request_animation_frame();

        let t = self.start.elapsed().as_secs_f32();
        div()
            .size_full()
            .bg(rgb(0x101418))
            .relative()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let w = f32::from(bounds.size.width) / BARS as f32;
                        let mid = f32::from(bounds.size.height) / 2.0;
                        for i in 0..BARS {
                            let x = i as f32 / BARS as f32 * 40.0 + t * 3.0;
                            let a = 0.5 * (x * 1.0).sin()
                                + 0.3 * (x * 2.7).sin()
                                + 0.2 * (x * 7.1).sin();
                            let h = (a.abs() * mid * 0.95).max(1.0);
                            let origin = point(
                                bounds.origin.x + px(i as f32 * w),
                                bounds.origin.y + px(mid - h),
                            );
                            window.paint_quad(fill(
                                Bounds::new(origin, size(px(w.max(1.0)), px(h * 2.0))),
                                rgb(0x3fb6ff),
                            ));
                        }
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top_2()
                    .left_2()
                    .text_color(rgb(0xffffff))
                    .child(format!("{:.1} fps", self.fps)),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.), px(400.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Waveform {
                    start: Instant::now(),
                    last: Instant::now(),
                    frames: 0,
                    fps: 0.0,
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
