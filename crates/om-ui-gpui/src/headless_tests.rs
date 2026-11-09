//! Headless GPUI tests: the real [`Shell`] rendered in gpui's test platform (no display, no GPU;
//! gpui's `test-support` feature, a dev-dependency only) over a real two-deck engine, at 1280x800
//! and 900x600. Each cell the shell lays out carries a debug selector (`app::cell_view`), so the
//! tests read back the bounds the elements were actually given and check them against the layout
//! model, then check that the knob, fader and meter painters plan non-empty geometry inside the
//! mixer column those bounds describe, and walk the keyboard through every control. Nothing here
//! opens an audio device.

use crate::app::{load_theme, RunOptions, SceneOptions, Shell};
use crate::controls::BarState;
use crate::fader_paint::{FaderFace, FaderLaw, FaderPlan};
use crate::knob_paint::knob_geometry;
use crate::meter_paint::{MeterFace, MeterPlan};
use gpui::{px, size, AppContext as _, Bounds, Entity, Pixels, TestAppContext, VisualTestContext};
use gpui_kit::base::Root;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::geometry::Rect;
use om_ui::header_layout::HeaderNeeds;
use om_ui::main_layout::{
    head_slot, page_for, resolve_density, CellKind, LayoutState, Page, MIXER_SLOT, SAMPLER_SLOT,
    STAGE_SLOT, TEMPO_SLOT,
};
use om_ui::theme::DensityChoice;
use om_ui::widgets::mixer::column_layout;
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
        media_keys: false,
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

/// What the shell lays out for `size`: the same call `Render` makes with no debug column and no
/// settings (so the Automatic density).
fn page(size: (i32, i32)) -> Page {
    let theme = load_theme();
    let needs = HeaderNeeds {
        automix: false,
        record: false,
        settings: false,
        notice: false,
    };
    let density = resolve_density(DensityChoice::Automatic, &theme, size);
    page_for(2, size, 0, LayoutState::default(), needs, (&theme, density))
}

fn tag(kind: &CellKind) -> &'static str {
    // Selectors are looked up by `&'static str`; the set of cells in a two-deck page is small.
    match kind {
        CellKind::Deck(0) => "deck:0",
        CellKind::Deck(1) => "deck:1",
        CellKind::Slot(n) if n == STAGE_SLOT => "slot:stage",
        CellKind::Slot(n) if n == MIXER_SLOT => "slot:mixer",
        CellKind::Slot(n) if n == SAMPLER_SLOT => "slot:sampler",
        CellKind::Slot(n) if n == TEMPO_SLOT => "slot:tempo",
        CellKind::Slot(n) if n == &head_slot(0) => "slot:head:0",
        CellKind::Slot(n) if n == &head_slot(1) => "slot:head:1",
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
            "slot:tempo",
            "slot:head:0",
            "slot:head:1",
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

/// The mixer column's knobs, faders and meters at each size, from the column layout of the cell the
/// mixer was actually rendered in.
#[gpui::test]
fn knob_fader_and_meter_painters_get_non_empty_bounds_in_the_rendered_mixer(
    cx: &mut TestAppContext,
) {
    let theme = load_theme();
    for size in SIZES {
        let vcx = open(cx, size);
        let card = to_rect(vcx.debug_bounds("slot:mixer").expect("mixer rendered"));
        let density = resolve_density(DensityChoice::Automatic, &theme, size);
        let m = *theme.metrics(density);
        let lay = column_layout(Rect::sized(card.w, card.h), &m);
        let inside = Rect::sized(card.w, card.h);
        for (ch, column) in lay.channels.iter().enumerate() {
            let name = format!("{size:?} channel {ch}");
            for (i, cell) in column.knobs.iter().enumerate() {
                let rect = cell.knob;
                assert!(
                    !rect.is_empty() && inside.contains(&rect),
                    "{name} knob {i}: {rect:?}"
                );
                let g = knob_geometry(rect.w as f32, rect.h as f32, 0.0, m.knob_size);
                assert!(
                    g.cap_diameter() >= 14.0,
                    "{name} knob {i}: cap {}",
                    g.cap_diameter()
                );
                let (x, y, side) = g.bbox();
                assert!(
                    x >= 0.0 && y >= 0.0 && x + side <= rect.w as f32 + 1.0,
                    "{name} knob {i}: leaves its cell"
                );
            }
            assert!(
                !column.fader.is_empty() && inside.contains(&column.fader),
                "{name}: fader"
            );
            let face = FaderFace::channel(FaderLaw::Channel).for_density(&m);
            let plan = FaderPlan::new(column.fader.w, column.fader.h, &face, 0.5);
            assert!(!plan.cap.is_empty(), "{name}: no fader cap");
            assert!(
                Rect::sized(column.fader.w, column.fader.h).contains(&plan.cap),
                "{name}: cap outside fader"
            );
            assert!(
                !column.meter.is_empty() && inside.contains(&column.meter),
                "{name}: meter"
            );
            let bars = [BarState {
                level: 0.8,
                hold: 0.9,
                clipping: false,
            }; 2];
            let plan = MeterPlan::new(
                &theme,
                column.meter,
                false,
                bars,
                MeterFace::channel(&theme),
            );
            let col = Rect::sized(column.meter.w, column.meter.h);
            for (i, side) in plan.sides.iter().enumerate() {
                assert!(
                    !side.well.is_empty() && col.contains(&side.well),
                    "{name} side {i}: well"
                );
                assert!(
                    !side.quads.is_empty(),
                    "{name} side {i}: nothing lit at 0.8"
                );
            }
            assert!(
                !plan.sides[0].well.intersects(&plan.sides[1].well),
                "{name}: wells overlap"
            );
        }
    }
}

/// The keyboard walk: Tab from the window's first stop reaches every control that registered a
/// tab stop (buttons, segments, pads, knobs, faders), and each one draws the 2px ring while it
/// has keyboard focus (the ring is the focus layer every stop carries, see `controls`).
#[gpui::test]
fn tab_reaches_every_control_of_the_window(cx: &mut TestAppContext) {
    let (_, vcx) = open_with(cx, (1440, 900), shell());
    let stops = crate::controls::registered_focus_handles();
    assert!(
        stops.len() >= 40,
        "only {} tab stops registered",
        stops.len()
    );
    let mut reached: Vec<gpui::FocusHandle> = Vec::new();
    vcx.update(|window, cx| {
        window.activate_window();
        for _ in 0..stops.len() * 3 {
            window.focus_next(cx);
            if let Some(focused) = window.focused(cx) {
                if !reached.contains(&focused) {
                    reached.push(focused);
                }
            }
        }
    });
    let missed: Vec<&String> = stops
        .iter()
        .filter(|(_, handle)| !reached.contains(handle))
        .map(|(id, _)| id)
        .collect();
    assert!(missed.is_empty(), "Tab never reaches {missed:?}");
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
        assert_eq!(theme.font_family.as_ref(), tokens.font_body);
        assert_eq!(theme.mono_font_family.as_ref(), tokens.font_mono);
        let rem = crate::theme_bridge::component_rem(&tokens);
        assert_eq!(f32::from(theme.font_size), rem);
        assert!(0.875 * rem >= tokens.type_body && 0.75 * rem >= tokens.type_label);
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

/// A shell whose settings ask the CLAP questions over a plugin host with CLAP support.
fn clap_shell(status: &Rc<std::cell::RefCell<om_ui::settings::ClapStatus>>) -> Shell {
    let status = Rc::clone(status);
    shell_with(move |options| {
        let mut o = fixtures::options(
            None,
            None,
            Some(Box::new(crate::settings_view::clap_tests::ClapHost {
                status: Rc::clone(&status),
            })),
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );
        o.ask_clap = true;
        options.settings = Some(o);
    })
}

#[gpui::test]
fn the_clap_questions_are_dialogs_asked_in_order_and_closed_by_their_answers(
    cx: &mut TestAppContext,
) {
    use crate::settings_view::overlay::Answer;
    use om_ui::preferences::ClapScan;
    let status = crate::settings_view::clap_tests::supported(1);
    let (view, vcx) = open_with(cx, SIZES[0], clap_shell(&status));
    vcx.run_until_parked();
    let asked = |view: &Entity<Shell>, vcx: &mut VisualTestContext| {
        view.read_with(vcx, |s, _| s.settings.as_ref().and_then(|s| s.ask_shown))
    };
    assert_eq!(asked(&view, vcx), Some(Ask::ClapFirstRun));
    assert!(vcx.update(|window, cx| window.has_active_dialog(cx)));

    // Answering the first-run question stores it; the warning is the next question.
    view.update(vcx, |shell, cx| {
        shell.resolve_ask(Ask::ClapFirstRun, Answer::Ok);
        cx.notify();
    });
    // The button closes its dialog itself; closing it here stands in for the click.
    vcx.update(|window, cx| window.close_dialog(cx));
    vcx.run_until_parked();
    let scan = view.read_with(vcx, |s, _| {
        s.settings.as_ref().map(|s| s.prefs.plugins.clap_scan)
    });
    assert_eq!(scan, Some(ClapScan::Allowed));
    assert_eq!(asked(&view, vcx), Some(Ask::ClapFirstLoad));
    assert!(vcx.update(|window, cx| window.has_active_dialog(cx)));

    // Cancelling the warning leaves the plugins unloaded and the dialog closed.
    view.update(vcx, |shell, cx| {
        shell.resolve_ask(Ask::ClapFirstLoad, Answer::Cancel);
        cx.notify();
    });
    vcx.update(|window, cx| window.close_dialog(cx));
    vcx.run_until_parked();
    assert_eq!(asked(&view, vcx), None);
    assert!(!vcx.update(|window, cx| window.has_active_dialog(cx)));
    let accepted = view.read_with(vcx, |s, _| {
        s.settings
            .as_ref()
            .map(|s| s.prefs.plugins.clap_warning_accepted)
    });
    assert_eq!(accepted, Some(false));
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

/// The loudness panel drawn at the rectangle it drops down to under the header toggle, for
/// `reading`, as a bare element so the shell's own tick (which polls the engine) cannot replace it.
fn draw_loudness_panel<'a>(
    cx: &'a mut TestAppContext,
    reading: &om_engine::LoudnessReading,
) -> (Rect, &'a mut VisualTestContext) {
    let theme = load_theme();
    let m = theme.metrics(om_ui::theme::Density::Compact);
    let header = om_ui::header_layout::header_rects(400, m, HeaderNeeds::default());
    let panel = om_ui::widgets::loudness::panel_rect(header.loudness, header.bar.bottom(), 3, 400);
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

// Desktop integration (#229): media keys, the clipboard, file-dialog picks and announcements.

use crate::media_keys::MediaSession;
use souvlaki::MediaControlEvent;

#[gpui::test]
fn the_play_pause_key_drives_the_lead_deck_and_the_widget_follows(cx: &mut TestAppContext) {
    let (session, os) = MediaSession::detached();
    let mut shell = shell();
    shell.media = Some(session);
    let (view, vcx) = open_with(cx, SIZES[0], shell);
    // Frame one publishes the idle state to the OS widget.
    let shown = view.read_with(vcx, |s, _| {
        s.media
            .as_ref()
            .and_then(|m| m.shown().map(|(t, a, p)| (t.to_owned(), a.to_owned(), p)))
    });
    assert_eq!(shown, Some((String::new(), String::new(), false)));

    os.send(MediaControlEvent::Toggle).unwrap();
    view.update(vcx, |_, cx| cx.notify());
    vcx.run_until_parked();
    // The key reached deck 1 through the router. The engine's audio side is dropped in this
    // test, so the deck cannot be heard to move; the command queue is what shows it was taken.
    assert!(
        view.read_with(vcx, |s, _| s.media.is_some()),
        "the session stays"
    );
    let drained = view.update(vcx, |s, _| s.media.as_mut().map(MediaSession::drain));
    assert_eq!(drained, Some(vec![]), "the frame took the key press");
}

#[gpui::test]
fn copy_puts_the_selected_tracks_on_the_clipboard(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], library_shell(&["Alpha", "Bravo"]));
    // Nothing selected: a hint, nothing copied.
    vcx.update(|window, cx| view.update(cx, |s, cx| s.copy_selected_tracks(window, cx)));
    assert!(vcx.read_from_clipboard().is_none());

    view.update(vcx, |shell, _| {
        shell.library.as_mut().expect("library").select_row(1);
    });
    vcx.update(|window, cx| view.update(cx, |s, cx| s.copy_selected_tracks(window, cx)));
    let text = vcx.read_from_clipboard().and_then(|item| item.text());
    let text = text.expect("the clipboard has text");
    assert!(text.starts_with("Tester - "), "{text}");
    assert!(text.contains("124 BPM"), "{text}");
    assert_eq!(text.lines().count(), 1);
}

#[gpui::test]
fn a_recording_that_just_stopped_is_announced_once(cx: &mut TestAppContext) {
    let mut shell = shell();
    shell.was_recording = true; // the last frame saw it running; the recorder is idle now
    let (_view, vcx) = open_with(cx, SIZES[0], shell);
    assert_eq!(vcx.update(|window, cx| window.notifications(cx).len()), 1);
    vcx.run_until_parked();
    assert_eq!(vcx.update(|window, cx| window.notifications(cx).len()), 1);
}

#[test]
fn a_picked_path_is_applied_as_if_typed() {
    let changes = Rc::new(std::cell::RefCell::new(0));
    let mut settings = fixtures::state(None, changes);
    // No library is attached, so the import is refused and says so.
    settings.apply_pick(EntryKind::ImportXml, std::path::Path::new("/music/lib.xml"));
    assert!(settings.notice.error, "{:?}", settings.notice);
    assert!(EntryKind::WatchFolder.can_browse());
    assert!(!EntryKind::MaxCacheGb.can_browse());
}

/// Focuses the sampler strip, as a click on it does.
fn focus_sampler(view: &Entity<Shell>, vcx: &mut VisualTestContext) {
    // The strip starts collapsed to a handle, which takes no pad keys; a click opens it.
    view.update(vcx, |shell, cx| {
        shell.sampler_open = true;
        cx.notify();
    });
    vcx.run_until_parked();
    let focus = view
        .read_with(vcx, |s, _| s.sampler_focus.clone())
        .expect("the sampler strip is drawn");
    vcx.update(|window, cx| focus.focus(window, cx));
    vcx.run_until_parked();
}

fn selected_pad(view: &Entity<Shell>, vcx: &mut VisualTestContext) -> usize {
    view.read_with(vcx, |s, _| s.sampler.selected())
}

/// Presses `pad`'s digit key (down and up) and returns the pad selected after.
fn press_pad_key(view: &Entity<Shell>, vcx: &mut VisualTestContext, pad: usize) -> usize {
    vcx.simulate_keystrokes(&(pad + 1).to_string());
    vcx.run_until_parked();
    selected_pad(view, vcx)
}

fn search_query(view: &Entity<Shell>, vcx: &mut VisualTestContext) -> Option<String> {
    view.read_with(vcx, |s, _| {
        s.library
            .as_ref()
            .map(|p| p.browser.search_text().to_owned())
    })
}

fn search_focused(view: &Entity<Shell>, vcx: &mut VisualTestContext) -> bool {
    let input = view
        .read_with(vcx, |s, _| s.library_search.clone())
        .expect("the search box is made on the first draw");
    vcx.update(|window, cx| input.focus_handle(cx).is_focused(window))
}

#[gpui::test]
fn keys_1_to_8_press_their_pads_while_the_strip_has_focus(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], library_shell(&["A"]));
    focus_sampler(&view, vcx);
    // Descending then ascending, so every key moves the selection to a pad it was not on.
    for pad in (0..8).rev().chain(0..8) {
        assert_eq!(press_pad_key(&view, vcx, pad), pad, "key {}", pad + 1);
        assert_eq!(
            view.read_with(vcx, |s, _| s.sampler.held()),
            None,
            "the key coming up released the pad"
        );
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
fn the_shell_starts_the_meter_and_the_toggle_opens_the_panel_under_the_header(
    cx: &mut TestAppContext,
) {
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
        let header = page(size).header;
        let panel = to_rect(vcx.debug_bounds("loudness:panel").expect("panel opened"));
        assert!(
            Rect::sized(size.0, size.1).contains(&panel),
            "{size:?}: {panel:?} leaves the window"
        );
        assert!(
            panel.y >= header.bar.bottom(),
            "{size:?}: {panel:?} over the header"
        );
        assert!(
            panel.right() <= header.loudness.right().max(size.0),
            "{size:?}"
        );
    }
}

#[gpui::test]
fn pad_keys_are_typed_text_not_triggers_while_the_search_box_has_focus(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], library_shell(&["A"]));
    focus_sampler(&view, vcx);
    assert_eq!(press_pad_key(&view, vcx, 3), 3);

    let input = view
        .read_with(vcx, |s, _| s.library_search.clone())
        .expect("the search box is made on the first draw");
    vcx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
    vcx.simulate_input("5");
    vcx.simulate_keystrokes("6");
    vcx.run_until_parked();
    assert_eq!(selected_pad(&view, vcx), 3, "no pad was triggered");
    assert_eq!(view.read_with(vcx, |s, _| s.sampler.held()), None);
    assert_eq!(search_query(&view, vcx).as_deref(), Some("56"));
}

#[gpui::test]
fn pad_keys_do_not_trigger_while_the_settings_text_field_has_focus(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], settings_shell());
    focus_sampler(&view, vcx);
    assert_eq!(press_pad_key(&view, vcx, 2), 2);

    view.update(vcx, |shell, cx| {
        shell.settings_mut(|s| s.begin_entry(EntryKind::WatchFolder));
        cx.notify();
    });
    vcx.run_until_parked();
    vcx.simulate_input("7");
    vcx.simulate_keystrokes("8");
    vcx.run_until_parked();
    assert_eq!(selected_pad(&view, vcx), 2, "no pad was triggered");
    let typed = view.read_with(vcx, |s, _| {
        s.settings
            .as_ref()
            .and_then(|s| s.entry.as_ref())
            .map(|e| e.text.clone())
    });
    assert_eq!(typed.as_deref(), Some("78"), "the digits went to the field");
}

#[gpui::test]
fn slash_focuses_the_search_box_and_escape_clears_it(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, SIZES[0], library_shell(&["House", "Techno"]));
    // The library card has focus, as after a click on it.
    let card = view
        .read_with(vcx, |s, _| s.library_focus.clone())
        .expect("the library card is drawn");
    vcx.update(|window, cx| card.focus(window, cx));
    vcx.run_until_parked();
    assert!(!search_focused(&view, vcx));

    vcx.simulate_keystrokes("/");
    vcx.run_until_parked();
    assert!(search_focused(&view, vcx), "/ focuses the search box");
    assert_eq!(search_query(&view, vcx).as_deref(), Some(""), "no / typed");

    vcx.simulate_input("tech");
    vcx.run_until_parked();
    assert_eq!(search_query(&view, vcx).as_deref(), Some("tech"));

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert_eq!(search_query(&view, vcx).as_deref(), Some(""));
    assert!(
        !search_focused(&view, vcx),
        "Escape gives the keyboard back"
    );
}

/// Tab is the keyboard walk: once a control has keyboard focus, Tab moves the focus on and does
/// not also hide the library band.
#[gpui::test]
fn tab_walks_the_focus_without_toggling_the_library(cx: &mut TestAppContext) {
    let (view, vcx) = open_with(cx, (1440, 900), shell());
    vcx.update(|window, cx| {
        window.activate_window();
        window.focus_next(cx);
    });
    let before = vcx.update(|window, cx| window.focused(cx));
    assert!(before.is_some(), "a first stop takes focus");
    vcx.simulate_keystrokes("tab");
    vcx.run_until_parked();
    let after = vcx.update(|window, cx| window.focused(cx));
    assert_ne!(before, after, "Tab moves the focus");
    assert!(
        !view.read_with(vcx, |s, _| s.performance),
        "Tab while walking does not hide the library"
    );
}
