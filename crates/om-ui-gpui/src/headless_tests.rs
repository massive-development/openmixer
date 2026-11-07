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
use gpui::{px, size, AppContext as _, Bounds, Entity, Pixels, TestAppContext, VisualTestContext};
use gpui_kit::base::Root;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::geometry::Rect;
use om_ui::header_layout::HeaderNeeds;
use om_ui::main_layout::{
    channel_slot, page_for, CellKind, LayoutState, Page, MIXER_SLOT, SAMPLER_SLOT, STAGE_SLOT,
};
use om_ui::widgets::channel::ChannelStrip;
use om_ui::widgets::mixer::MixerPanel;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

const SIZES: [(i32, i32); 2] = [(1280, 800), (900, 600)];

fn shell() -> Shell {
    shell_with(|_| {})
}

/// A shell whose window options `tweak` has changed (a library, settings, a banner).
fn shell_with(tweak: impl FnOnce(&mut RunOptions)) -> Shell {
    let (engine, _audio) = DjEngine::new(2);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let mut options = RunOptions {
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
    tweak(&mut options);
    Shell::new(
        engine,
        router,
        load_theme(),
        options,
        Arc::new(AtomicU64::new(0)),
    )
}

/// The shell in a test window of `(w, h)` px, drawn once.
fn open(cx: &mut TestAppContext, size: (i32, i32)) -> &mut VisualTestContext {
    open_with(cx, size, shell()).1
}

/// `shell` in a test window under a component `Root`, as `run` opens it: the component library is
/// initialised and themed from the shell's tokens first.
fn open_with(
    cx: &mut TestAppContext,
    (w, h): (i32, i32),
    shell: Shell,
) -> (Entity<Shell>, &mut VisualTestContext) {
    let theme = shell.theme.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::theme_bridge::apply(cx, &theme);
    });
    let view = cx.new(|_| shell);
    let root = view.clone();
    let (_root, vcx) = cx.add_window_view(|window, cx| Root::new(root, window, cx));
    vcx.simulate_resize(gpui::size(px(w as f32), px(h as f32)));
    vcx.run_until_parked();
    (view, vcx)
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

// The component chrome: themed buttons, the library's input and table, the settings text field,
// the confirmation dialog and the toasts. These drive real key and text events through the
// component widgets in gpui's test platform; they say nothing about how the widgets look.

use crate::debug_panel::DeviceStatus;
use crate::library::LibraryHub;
use crate::settings_view::overlay::Ask;
use crate::settings_view::{fixtures, EntryKind};
use gpui::{Focusable as _, Hsla};
use gpui_kit::component::table::TableDelegate as _;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use om_library::TrackRecord;
use om_ui::library::{LibraryServices, Options};
use std::cell::Cell;
use std::rc::Rc;

/// A shell over an in-memory library holding one track per title.
fn library_shell(titles: &[&str]) -> Shell {
    let services =
        LibraryServices::create(Options::for_path(":memory:")).expect("in-memory library opens");
    let hub = LibraryHub::shared(services, 2);
    for title in titles {
        let track = TrackRecord {
            path: format!("/nowhere/{title}.wav"),
            title: (*title).to_owned(),
            artist: "Tester".to_owned(),
            bpm: Some(124.0),
            duration_sec: 1.0,
            ..TrackRecord::default()
        };
        hub.borrow()
            .services
            .library()
            .tracks()
            .insert(&track)
            .expect("track inserts");
    }
    shell_with(|options| options.library = Some(hub))
}

/// A shell with the settings overlay open on the Library tab.
fn settings_shell() -> Shell {
    shell_with(|options| {
        let mut settings = fixtures::options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );
        settings.start_tab = Some("library".to_owned());
        options.settings = Some(settings);
    })
}

fn component_colour(colour: om_ui::theme::Colour) -> Hsla {
    Hsla::from(crate::paint::paint(colour))
}

#[gpui::test]
fn the_component_theme_follows_the_om_ui_tokens(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], shell());
    let tokens = view.read_with(vcx, |s, _| s.theme.clone());
    vcx.update(|_, cx| {
        let theme = cx.theme();
        assert_eq!(theme.background, component_colour(tokens.surface));
        assert_eq!(theme.foreground, component_colour(tokens.text_primary));
        assert_eq!(theme.input, component_colour(tokens.border_strong));
        assert_eq!(
            theme.is_dark(),
            crate::theme_bridge::mode_of(&tokens).is_dark()
        );
    });
}

#[gpui::test]
fn the_banner_is_a_toast_that_stays(cx: &mut TestAppContext) {
    let shell = shell_with(|options| options.banner = Some("Audio: mock device".to_owned()));
    let (view, vcx) = open_with(cx, SIZES[0], shell);
    assert_eq!(vcx.update(|window, cx| window.notifications(cx).len()), 1);
    assert!(
        view.read_with(vcx, |s, _| s.options.banner.is_none()),
        "shown once"
    );
    vcx.run_until_parked();
    assert_eq!(vcx.update(|window, cx| window.notifications(cx).len()), 1);
}

#[gpui::test]
fn the_library_table_lists_the_tracks_and_the_search_box_narrows_it(cx: &mut TestAppContext) {
    let shell = library_shell(&["House Anthem", "Techno Pulse", "Deep House"]);
    let (view, vcx) = open_with(cx, SIZES[0], shell);
    let table = view
        .read_with(vcx, |s, _| s.library_table.clone())
        .expect("table drawn");
    let rows =
        |vcx: &mut VisualTestContext| vcx.update(|_, cx| table.read(cx).delegate().rows_count(cx));
    assert_eq!(rows(vcx), 3);

    let input = view
        .read_with(vcx, |s, _| s.library_search.clone())
        .expect("the search box is made on the first draw");
    vcx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
    vcx.simulate_input("house");
    vcx.run_until_parked();
    let (query, typing) = vcx.update(|window, cx| {
        let shell = view.read(cx);
        (
            shell
                .library
                .as_ref()
                .map(|p| p.browser.search_text().to_owned()),
            shell.typing(window, cx),
        )
    });
    assert_eq!(query.as_deref(), Some("house"));
    assert!(typing, "typed keys in the box must not reach the shortcuts");
    // The query runs on a tick, which the draw after the keystroke made.
    vcx.run_until_parked();
    assert_eq!(rows(vcx), 2, "House Anthem and Deep House");

    // Escape drops the query and gives the keyboard back to the library card.
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert_eq!(
        view.read_with(vcx, |s, _| s
            .library
            .as_ref()
            .map(|p| p.browser.search_text().to_owned())),
        Some(String::new())
    );
    assert_eq!(rows(vcx), 3);
}

#[gpui::test]
fn naming_a_playlist_uses_the_search_box_and_enter_creates_it(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], library_shell(&["A", "B"]));
    view.update(vcx, |shell, cx| {
        shell
            .library
            .as_mut()
            .expect("library")
            .begin_new_playlist();
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(
        view.read_with(vcx, |s, _| s.search_names),
        "the box shows the name prompt"
    );

    vcx.simulate_input("Warm up");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    let (naming, shown, playlist) = view.read_with(vcx, |s, _| {
        let panel = s.library.as_ref().expect("library");
        (
            panel.name_entry.is_some(),
            s.search_names,
            panel.browser.selected_playlist(),
        )
    });
    assert!(!naming, "Enter created the playlist");
    assert!(!shown, "the box is a search box again");
    assert!(playlist.is_some(), "the new playlist is open");
}

#[gpui::test]
fn the_settings_text_field_is_an_input_that_escape_cancels(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], settings_shell());
    view.update(vcx, |shell, cx| {
        shell.settings_mut(|s| s.begin_entry(EntryKind::WatchFolder));
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(
        view.read_with(vcx, |s, _| s
            .settings
            .as_ref()
            .is_some_and(|s| s.entry_input.is_some())),
        "the field has an input"
    );

    vcx.simulate_input("/music");
    vcx.run_until_parked();
    let typed = view.read_with(vcx, |s, _| {
        s.settings
            .as_ref()
            .and_then(|s| s.entry.as_ref())
            .map(|e| e.text.clone())
    });
    assert_eq!(typed.as_deref(), Some("/music"), "typing edits the entry");

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    let (entry, input, open) = view.read_with(vcx, |s, _| {
        let s = s.settings.as_ref().expect("settings");
        (s.entry.is_some(), s.entry_input.is_some(), s.open)
    });
    assert!(
        !entry && !input,
        "Escape cancels the field and drops its input"
    );
    assert!(open, "the first Escape only cancelled the field");
}

#[gpui::test]
fn a_confirmation_is_a_dialog_that_closes_when_the_settings_stop_asking(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], settings_shell());
    view.update(vcx, |shell, cx| {
        shell.settings_mut(|s| s.stems_ask_clear());
        cx.notify();
    });
    vcx.run_until_parked();
    let asked = view.read_with(vcx, |s, _| s.settings.as_ref().and_then(|s| s.ask_shown));
    assert_eq!(asked, Some(Ask::ClearStems));
    assert!(vcx.update(|window, cx| window.has_active_dialog(cx)));

    view.update(vcx, |shell, cx| {
        shell.settings_mut(|s| s.stems_clear_cancelled());
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(!vcx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui::test]
fn each_notice_is_one_toast_even_when_the_text_repeats(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], settings_shell());
    for expected in 1..=2 {
        view.update(vcx, |shell, cx| {
            shell.settings_mut(|s| s.set_notice("Imported 3 tracks", false));
            cx.notify();
        });
        vcx.run_until_parked();
        assert_eq!(
            vcx.update(|window, cx| window.notifications(cx).len()),
            expected
        );
    }
}

#[gpui::test]
fn the_debug_columns_path_field_loads_on_enter(cx: &mut TestAppContext) {
    let shell = shell_with(|options| {
        options.debug = Some(Arc::new(|| DeviceStatus {
            name: "mock".into(),
            sample_rate: 48_000.0,
            buffer_size: 256,
            round_trip_ms: 5.0,
            callback_avg_us: 100.0,
            callback_max_us: 200.0,
            budget_us: 5_000.0,
            cpu_load: 0.02,
            xruns: 0,
            callbacks: 10,
        }));
    });
    let (view, vcx) = open_with(cx, SIZES[0], shell);
    let first = view
        .read_with(vcx, |s, _| s.deck_controls.inputs().next().cloned())
        .expect("a path field per deck");
    vcx.update(|window, cx| first.focus_handle(cx).focus(window, cx));
    vcx.simulate_input("/nowhere/a.wav");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    let status = view.read_with(vcx, |s, _| s.deck_controls.status.clone());
    let (text, error) = status.expect("Enter reported an outcome");
    assert!(error, "there is no library, so the load is refused: {text}");
}

fn loud_reading() -> om_engine::LoudnessReading {
    om_engine::LoudnessReading {
        momentary_lufs: -9.5,
        short_term_lufs: -12.0,
        integrated_lufs: -13.1,
        loudness_range_lu: 6.2,
        true_peak_dbtp: [-2.0, 0.4],
        dropped_frames: 0,
        frames_measured: 480_000,
    }
}

/// The loudness panel drawn at its mixer-module rectangle for `reading`, as a bare element so the
/// shell's own tick (which polls the engine) cannot replace the reading.
fn draw_loudness_panel<'a>(
    cx: &'a mut TestAppContext,
    reading: &om_engine::LoudnessReading,
) -> (Rect, &'a mut VisualTestContext) {
    let card = Rect::sized(253, 262);
    let panel = MixerPanel::module_layout(card, &load_theme()).loudness_panel;
    let shell = {
        let (view, vcx) = open_with(cx, SIZES[0], shell());
        view.update(vcx, |shell, _| shell.loudness.set_reading(reading));
        view
    };
    let (_, vcx) = cx.add_window_view(|_, _| PanelOnly { shell, panel });
    vcx.simulate_resize(gpui::size(px(400.), px(400.)));
    vcx.run_until_parked();
    (panel, vcx)
}

/// A view that renders only the loudness panel of `shell`, which it does not tick.
struct PanelOnly {
    shell: Entity<Shell>,
    panel: Rect,
}

impl gpui::Render for PanelOnly {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let panel = self.panel;
        self.shell.update(cx, |shell, cx| {
            crate::loudness_paint::loudness_panel(shell, panel, cx)
        })
    }
}

#[gpui::test]
fn the_loudness_panel_draws_a_reading_inside_its_slot(cx: &mut TestAppContext) {
    let (panel, vcx) = draw_loudness_panel(cx, &loud_reading());
    let got = to_rect(vcx.debug_bounds("loudness:panel").expect("panel drawn"));
    assert_eq!(got, panel);
    assert!(
        vcx.debug_bounds("loudness:lamp:lit").is_some(),
        "0.4 dBTP clips"
    );
    assert!(vcx.debug_bounds("loudness:warning").is_none());
}

#[gpui::test]
fn the_loudness_panel_warns_when_frames_were_dropped(cx: &mut TestAppContext) {
    let mut reading = loud_reading();
    reading.true_peak_dbtp = [-6.0, -6.0];
    reading.dropped_frames = 960;
    let (_, vcx) = draw_loudness_panel(cx, &reading);
    assert!(vcx.debug_bounds("loudness:warning").is_some());
    assert!(vcx.debug_bounds("loudness:lamp:off").is_some());
}

#[gpui::test]
fn the_loudness_panel_draws_before_anything_is_measured(cx: &mut TestAppContext) {
    let (_, vcx) = draw_loudness_panel(cx, &om_engine::LoudnessReading::SILENT);
    assert!(vcx.debug_bounds("loudness:panel").is_some());
    assert!(vcx.debug_bounds("loudness:lamp:off").is_some());
}

#[gpui::test]
fn the_shell_starts_the_meter_and_the_toggle_opens_the_panel_in_the_mixer(cx: &mut TestAppContext) {
    for size in SIZES {
        let (view, vcx) = open_with(cx, size, shell());
        assert!(view.read_with(vcx, |s, _| s.engine.loudness().is_enabled()));
        assert!(
            vcx.debug_bounds("loudness:panel").is_none(),
            "closed at first"
        );
        view.update(vcx, |s, cx| {
            s.loudness.toggle();
            cx.notify();
        });
        vcx.run_until_parked();
        let mixer = to_rect(vcx.debug_bounds("slot:mixer").expect("mixer"));
        let panel = to_rect(vcx.debug_bounds("loudness:panel").expect("panel opened"));
        assert!(
            mixer.contains(&panel),
            "{size:?}: {panel:?} outside {mixer:?}"
        );
    }
}
