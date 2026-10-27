//! Headless GPUI tests: the real [`Shell`] rendered in gpui's test platform (no display, no GPU;
//! gpui's `test-support` feature, a dev-dependency only) over a real two-deck engine, at 1280x800
//! and 900x600. Each cell the shell lays out carries a debug selector (`app::cell_view`), so the
//! tests read back the bounds the elements were actually given and check them against the layout
//! model, then check that the knob, fader and meter painters plan non-empty geometry inside the
//! channel strip those bounds describe. Nothing here opens an audio device.

use crate::app::{load_theme, RunOptions, SceneOptions, Shell};
use crate::controls::BarState;
use crate::fader_paint::{FaderFace, FaderLaw, FaderPlan};
use crate::knob_paint::{knob_geometry, KnobRole};
use crate::meter_paint::{MeterFace, MeterPlan};
use gpui::{px, size, Bounds, Pixels, TestAppContext, VisualTestContext};
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::geometry::Rect;
use om_ui::header_layout::HeaderNeeds;
use om_ui::main_layout::{
    channel_slot, page_for, CellKind, LayoutState, Page, MIXER_SLOT, SAMPLER_SLOT, STAGE_SLOT,
};
use om_ui::widgets::channel::ChannelStrip;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

const SIZES: [(i32, i32); 2] = [(1280, 800), (900, 600)];

fn shell() -> Shell {
    let (engine, _audio) = DjEngine::new(2);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let options = RunOptions {
        on_frame: None,
        automix: None,
        record: false,
        smoke_frames: None,
        banner: None,
        debug: None,
        settings: None,
        library: None,
        library_rows_out: None,
        demo_grid_bpm: None,
        samples: None,
        scene: SceneOptions::default(),
    };
    Shell::new(
        engine,
        router,
        load_theme(),
        options,
        Arc::new(AtomicU64::new(0)),
    )
}

/// The shell in a test window of `(w, h)` px, drawn once.
fn open(cx: &mut TestAppContext, (w, h): (i32, i32)) -> &mut VisualTestContext {
    let (_view, vcx) = cx.add_window_view(|_, _| shell());
    vcx.simulate_resize(size(px(w as f32), px(h as f32)));
    vcx.run_until_parked();
    vcx
}

/// What the shell lays out for `size`: the same call `Render` makes with no debug column.
fn page(size: (i32, i32)) -> Page {
    let theme = load_theme();
    let needs = HeaderNeeds {
        automix: false,
        record: false,
        settings: false,
        notice: false,
    };
    page_for(2, size, 0, theme.gap, LayoutState::default(), needs)
}

fn tag(kind: &CellKind) -> &'static str {
    // Selectors are looked up by `&'static str`; the set of cells in a two-deck page is small.
    match kind {
        CellKind::Deck(0) => "deck:0",
        CellKind::Deck(1) => "deck:1",
        CellKind::Slot(n) if n == STAGE_SLOT => "slot:stage",
        CellKind::Slot(n) if n == MIXER_SLOT => "slot:mixer",
        CellKind::Slot(n) if n == SAMPLER_SLOT => "slot:sampler",
        CellKind::Slot(n) if n == &channel_slot(0) => "slot:channel:0",
        CellKind::Slot(n) if n == &channel_slot(1) => "slot:channel:1",
        CellKind::Slot(n) if n == "library" => "slot:library",
        other => panic!("unexpected cell {other:?}"),
    }
}

fn to_rect(b: Bounds<Pixels>) -> Rect {
    Rect::new(
        f32::from(b.origin.x).round() as i32,
        f32::from(b.origin.y).round() as i32,
        f32::from(b.size.width).round() as i32,
        f32::from(b.size.height).round() as i32,
    )
}

#[gpui::test]
fn every_cell_renders_at_the_bounds_the_layout_gave_it(cx: &mut TestAppContext) {
    for size in SIZES {
        let vcx = open(cx, size);
        let page = page(size);
        assert!(
            page.cells.len() >= 6,
            "{size:?}: only {} cells",
            page.cells.len()
        );
        for cell in &page.cells {
            let name = tag(&cell.kind);
            let got = vcx
                .debug_bounds(name)
                .unwrap_or_else(|| panic!("{size:?}: {name} did not render"));
            assert_eq!(to_rect(got), cell.bounds, "{size:?}: {name}");
        }
        // The window itself reached the shell at the size it was given.
        let window = vcx.update(|w, _| w.viewport_size());
        assert_eq!(window, gpui::size(px(size.0 as f32), px(size.1 as f32)));
    }
}

#[gpui::test]
fn the_key_cells_are_present_inside_the_window_and_never_overlap(cx: &mut TestAppContext) {
    for size in SIZES {
        let vcx = open(cx, size);
        let window = Rect::sized(size.0, size.1);
        let rendered: Vec<(&str, Rect)> = page(size)
            .cells
            .iter()
            .map(|c| {
                let name = tag(&c.kind);
                let b = vcx
                    .debug_bounds(name)
                    .unwrap_or_else(|| panic!("{name} missing"));
                (name, to_rect(b))
            })
            .collect();
        for key in [
            "deck:0",
            "deck:1",
            "slot:stage",
            "slot:mixer",
            "slot:channel:0",
            "slot:channel:1",
        ] {
            assert!(
                rendered.iter().any(|(n, _)| *n == key),
                "{size:?}: no {key}"
            );
        }
        for (name, r) in &rendered {
            assert!(!r.is_empty(), "{size:?}: {name} is empty");
            assert!(
                window.contains(r),
                "{size:?}: {name} {r:?} leaves the window"
            );
        }
        for (i, (a, ra)) in rendered.iter().enumerate() {
            for (b, rb) in &rendered[i + 1..] {
                assert!(
                    !ra.intersects(rb),
                    "{size:?}: {a} {ra:?} overlaps {b} {rb:?}"
                );
            }
        }
    }
}

/// The channel strips' knobs, faders and meters at each size, from the strip layout of the card
/// each channel was actually rendered in.
#[gpui::test]
fn knob_fader_and_meter_painters_get_non_empty_bounds_in_the_rendered_strips(
    cx: &mut TestAppContext,
) {
    let theme = load_theme();
    for size in SIZES {
        let vcx = open(cx, size);
        for ch in 0..2 {
            let name = if ch == 0 {
                "slot:channel:0"
            } else {
                "slot:channel:1"
            };
            let card = to_rect(vcx.debug_bounds(name).expect("strip rendered"));
            let lay = ChannelStrip::layout(Rect::sized(card.w, card.h), &theme);
            let inside = Rect::sized(card.w, card.h);

            // Knobs: trim, filter and the three bands get a real cap inside their cell.
            for (what, rect, role) in [
                ("trim", lay.trim, KnobRole::Wide),
                ("filter", lay.filter, KnobRole::Wide),
                ("eq0", lay.eq[0], KnobRole::Band),
                ("eq1", lay.eq[1], KnobRole::Band),
                ("eq2", lay.eq[2], KnobRole::Band),
            ] {
                assert!(
                    !rect.is_empty() && inside.contains(&rect),
                    "{size:?} {name} {what}: {rect:?}"
                );
                let g = knob_geometry(rect.w as f32, rect.h as f32, 0.0, role);
                assert!(
                    g.cap_diameter() >= 20.0,
                    "{size:?} {name} {what}: cap {}",
                    g.cap_diameter()
                );
                let (x, y, side) = g.bbox();
                assert!(
                    x >= 0.0 && y >= 0.0 && x + side <= rect.w as f32 + 1.0,
                    "{size:?} {name} {what}: knob leaves its cell"
                );
            }

            // Fader: a cap and tick marks inside the fader rect.
            assert!(
                !lay.fader.is_empty() && inside.contains(&lay.fader),
                "{size:?} {name}: fader"
            );
            let face = FaderFace::channel(FaderLaw::Channel);
            let plan = FaderPlan::new(lay.fader.w, lay.fader.h, &face, 0.5);
            assert!(!plan.cap.is_empty(), "{size:?} {name}: no fader cap");
            assert!(
                Rect::sized(lay.fader.w, lay.fader.h).contains(&plan.cap),
                "{size:?} {name}: cap outside fader"
            );
            assert!(!plan.ticks.is_empty(), "{size:?} {name}: no fader ticks");

            // Meter: two wells with canvases, lit segments at a high level and a lamp.
            assert!(
                !lay.meter.is_empty() && inside.contains(&lay.meter),
                "{size:?} {name}: meter"
            );
            let bars = [BarState {
                level: 0.8,
                hold: 0.9,
                clipping: false,
            }; 2];
            let m = MeterPlan::new(
                &theme,
                lay.meter,
                lay.meter_labels,
                bars,
                MeterFace::channel(&theme),
            );
            let col = Rect::sized(lay.meter.w, lay.meter.h);
            for (i, side) in m.sides.iter().enumerate() {
                assert!(
                    !side.well.is_empty() && col.contains(&side.well),
                    "{size:?} {name} side {i}: well"
                );
                assert!(
                    !side.area.is_empty() && side.well.contains(&side.area),
                    "{size:?} {name} side {i}: area"
                );
                assert!(
                    !side.lamp.is_empty() && col.contains(&side.lamp),
                    "{size:?} {name} side {i}: lamp"
                );
                assert!(
                    !side.quads.is_empty(),
                    "{size:?} {name} side {i}: nothing lit at 0.8"
                );
            }
            assert!(
                !m.sides[0].well.intersects(&m.sides[1].well),
                "{size:?} {name}: wells overlap"
            );
        }
    }
}

#[gpui::test]
fn a_resize_relayouts_the_same_shell(cx: &mut TestAppContext) {
    let vcx = open(cx, SIZES[0]);
    let wide = to_rect(vcx.debug_bounds("slot:mixer").expect("mixer"));
    vcx.simulate_resize(size(px(SIZES[1].0 as f32), px(SIZES[1].1 as f32)));
    vcx.run_until_parked();
    let narrow = to_rect(vcx.debug_bounds("slot:mixer").expect("mixer"));
    assert_ne!(
        wide, narrow,
        "the mixer card kept its bounds across a resize"
    );
    assert_eq!(
        narrow,
        page(SIZES[1])
            .cells
            .iter()
            .find(|c| tag(&c.kind) == "slot:mixer")
            .expect("cell")
            .bounds
    );
}
