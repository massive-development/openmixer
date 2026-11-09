//! The main window's rect contract (the faceplate, comp 4a/4b) at every window size the app must
//! lay out, in both densities, plus the generic data-driven grid.

use om_ui::geometry::Rect;
use om_ui::header_layout::HeaderNeeds;
use om_ui::main_layout::{
    compute_layout, deck_of_head, head_slot, page_for, resolve_density, visible_cells, CellKind,
    LayoutSpec, LayoutState, Page, LIBRARY_SLOT, MIN_WINDOW, MIXER_SLOT, SAMPLER_SLOT, STAGE_SLOT,
    TEMPO_SLOT,
};
use om_ui::theme::{Density, DensityChoice, ThemeTokens};
use om_ui::widgets::deck::deck_controls_layout;
use om_ui::widgets::deck_header::head_layout;
use om_ui::widgets::mixer::column_layout;
use om_ui::widgets::stage::stage_layout;

/// Window sizes the app must lay out cleanly: the 1280x720 minimum (`MIN_WINDOW`), the comp's
/// 1440x900, full HD, and two sizes under the minimum that must still not overlap.
const WINDOWS: [(i32, i32); 5] = [
    (1280, 720),
    (900, 600),
    (1024, 640),
    (1440, 900),
    (1920, 1080),
];
const DENSITIES: [Density; 2] = [Density::Roomy, Density::Compact];
const FULL: HeaderNeeds = HeaderNeeds {
    automix: true,
    record: true,
    settings: true,
    notice: false,
};

/// Every state the page has: library shown or hidden, sampler open or collapsed.
fn states() -> Vec<LayoutState> {
    let mut out = Vec::new();
    for performance in [false, true] {
        for sampler_open in [false, true] {
            out.push(LayoutState {
                performance,
                sampler_open,
                sampler_visible: true,
            });
        }
    }
    out
}

fn page(window: (i32, i32), density: Density, state: LayoutState) -> Page {
    page_for(
        2,
        window,
        0,
        state,
        FULL,
        (&ThemeTokens::builtin_dark(), density),
    )
}

fn each_page(mut check: impl FnMut((i32, i32), Density, LayoutState, &Page)) {
    for window in WINDOWS {
        for density in DENSITIES {
            for state in states() {
                check(window, density, state, &page(window, density, state));
            }
        }
    }
}

fn cell(page: &Page, kind: CellKind) -> Rect {
    page.cell(&kind)
        .unwrap_or_else(|| panic!("no {kind:?} cell"))
}

fn slot(name: &str) -> CellKind {
    CellKind::Slot(name.to_owned())
}

fn assert_disjoint(label: &str, parts: &[Rect]) {
    for (i, a) in parts.iter().enumerate() {
        for b in &parts[i + 1..] {
            assert!(!a.intersects(b), "{label}: {a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn every_cell_is_inside_the_window_under_the_header_and_none_overlap() {
    each_page(|(w, h), density, state, page| {
        let label = format!("{w}x{h} {density:?} {state:?}");
        let window = Rect::sized(w, h);
        let bar = page.header.all();
        for part in &bar {
            assert!(
                page.header.bar.contains(part),
                "{label}: {part:?} leaves the header"
            );
        }
        assert_disjoint(&label, &bar);
        let cells: Vec<Rect> = page.cells.iter().map(|c| c.bounds).collect();
        for c in &page.cells {
            assert!(
                window.contains(&c.bounds),
                "{label}: {:?} leaves the window",
                c.kind
            );
            assert!(
                page.area.contains(&c.bounds),
                "{label}: {:?} leaves the area",
                c.kind
            );
            assert!(
                c.bounds.y >= page.header.bar.bottom(),
                "{label}: {:?}",
                c.kind
            );
        }
        assert_disjoint(&label, &cells);
    });
}

#[test]
fn the_rows_run_header_heads_stage_controls_library() {
    each_page(|(w, h), density, state, page| {
        let label = format!("{w}x{h} {density:?} {state:?}");
        let head = cell(page, CellKind::Slot(head_slot(0)));
        let stage = cell(page, slot(STAGE_SLOT));
        let deck = cell(page, CellKind::Deck(0));
        assert!(head.bottom() <= stage.y, "{label}: heads above the stage");
        assert!(
            stage.bottom() <= deck.y,
            "{label}: stage above the controls"
        );
        for d in 0..2 {
            assert_eq!(cell(page, CellKind::Deck(d)).y, deck.y, "{label}");
            assert_eq!(
                cell(page, CellKind::Slot(head_slot(d))).y,
                head.y,
                "{label}"
            );
        }
        assert_eq!(cell(page, slot(MIXER_SLOT)).y, deck.y);
        assert_eq!(cell(page, slot(TEMPO_SLOT)).y, head.y);
        if state.performance {
            assert!(page.cell(&slot(LIBRARY_SLOT)).is_none(), "{label}");
            return;
        }
        let library = cell(page, slot(LIBRARY_SLOT));
        for c in &page.cells {
            if c.kind != slot(LIBRARY_SLOT) && c.kind != slot(SAMPLER_SLOT) {
                assert!(
                    c.bounds.bottom() <= library.y,
                    "{label}: {:?} reaches into the library band",
                    c.kind
                );
            }
        }
    });
}

#[test]
fn deck_b_mirrors_deck_a_about_the_mixer() {
    each_page(|(w, h), density, state, page| {
        let label = format!("{w}x{h} {density:?} {state:?}");
        let (a, mixer, b) = (
            cell(page, CellKind::Deck(0)),
            cell(page, slot(MIXER_SLOT)),
            cell(page, CellKind::Deck(1)),
        );
        assert!(a.right() <= mixer.x && mixer.right() <= b.x, "{label}");
        assert!((a.w - b.w).abs() <= 1, "{label}: decks {} and {}", a.w, b.w);
        assert_eq!(
            mixer.x - a.right(),
            b.x - mixer.right(),
            "{label}: equal gaps"
        );
        let tempo = cell(page, slot(TEMPO_SLOT));
        assert_eq!(
            (tempo.x, tempo.w),
            (mixer.x, mixer.w),
            "{label}: tempo over the mixer"
        );
    });
}

#[test]
fn the_mixer_column_is_320_roomy_and_260_compact_from_the_minimum_window() {
    for window in WINDOWS.into_iter().filter(|w| w.0 >= MIN_WINDOW.0) {
        for (density, width) in [(Density::Roomy, 320), (Density::Compact, 260)] {
            let p = page(window, density, LayoutState::default());
            assert_eq!(
                cell(&p, slot(MIXER_SLOT)).w,
                width,
                "{window:?} {density:?}"
            );
        }
    }
}

#[test]
fn the_parts_of_every_deck_head_and_deck_fit_their_cells() {
    let t = ThemeTokens::builtin_dark();
    each_page(|(w, h), density, state, page| {
        let label = format!("{w}x{h} {density:?} {state:?}");
        let m = t.metrics(density);
        for d in 0..2 {
            let mirrored = d == 1;
            let deck = cell(page, CellKind::Deck(d));
            let l = deck_controls_layout(deck, m, mirrored);
            let parts = [l.pitch, l.transport, l.tabs, l.pads, l.options];
            for p in &parts {
                assert!(
                    deck.contains(p),
                    "{label}: deck {d} part {p:?} leaves {deck:?}"
                );
            }
            assert_disjoint(&label, &parts);
            let head = cell(page, CellKind::Slot(head_slot(d)));
            let hl = head_layout(head, &t, m, mirrored);
            let parts = [
                hl.tag,
                hl.cover,
                hl.title,
                hl.readouts[0],
                hl.readouts[1],
                hl.readouts[2],
                hl.overview,
            ];
            for p in &parts {
                assert!(
                    head.contains(p),
                    "{label}: head {d} part {p:?} leaves {head:?}"
                );
            }
            assert_disjoint(&label, &parts);
        }
    });
}

#[test]
fn the_mixer_column_fits_its_cell() {
    let t = ThemeTokens::builtin_dark();
    each_page(|(w, h), density, state, page| {
        let label = format!("{w}x{h} {density:?} {state:?}");
        let mixer = cell(page, slot(MIXER_SLOT));
        let l = column_layout(mixer, t.metrics(density));
        let mut parts = vec![
            l.master_fader,
            l.master_meter,
            l.crossfader,
            l.curve,
            l.tags[0],
            l.tags[1],
            l.phones_mix.bounds(),
            l.phones_level.bounds(),
        ];
        for c in &l.channels {
            parts.extend(c.knobs.iter().map(|k| k.bounds()));
            parts.extend([c.fader, c.meter, c.cue, c.assign]);
        }
        for p in &parts {
            assert!(
                mixer.contains(p),
                "{label}: mixer part {p:?} leaves {mixer:?}"
            );
        }
        assert_disjoint(&label, &parts);
    });
}

/// At the sizes the app opens at (the minimum and up) with the density Automatic picks, both
/// waveforms are drawn, and with the library shown it has at least four rows.
#[test]
fn the_waveforms_and_the_library_stay_usable_from_the_minimum_window() {
    let t = ThemeTokens::builtin_dark();
    for window in WINDOWS.into_iter().filter(|w| w.0 >= MIN_WINDOW.0) {
        let density = resolve_density(DensityChoice::Automatic, &t, window);
        let p = page(window, density, LayoutState::default());
        let stage = cell(&p, slot(STAGE_SLOT));
        let strips = stage_layout(stage, 2);
        assert_eq!(strips.len(), 2, "{window:?} {density:?}: {stage:?}");
        assert!(strips.iter().all(|s| stage.contains(s)));
        let library = cell(&p, slot(LIBRARY_SLOT));
        let metrics = om_ui::main_layout::library_metrics(t.metrics(density));
        assert!(
            library.h >= metrics.min_height(4),
            "{window:?}: library {library:?}"
        );
    }
}

#[test]
fn the_header_holds_its_parts_and_the_automix_bar_from_the_minimum_width() {
    for window in WINDOWS.into_iter().filter(|w| w.0 >= MIN_WINDOW.0) {
        for density in DENSITIES {
            let p = page(window, density, LayoutState::default());
            let bar = p.header.automix.expect("automix bar");
            assert!(bar.w >= 300, "{window:?} {density:?}: bar is {} px", bar.w);
            assert!(p.header.record.is_some() && p.header.settings.is_some());
        }
    }
}

#[test]
fn head_slot_names_round_trip() {
    assert_eq!(deck_of_head(&head_slot(0)), Some(0));
    assert_eq!(deck_of_head(STAGE_SLOT), None);
}

#[test]
fn a_one_deck_engine_places_no_deck_b() {
    let p = page_for(
        1,
        (1440, 900),
        0,
        LayoutState::default(),
        FULL,
        (&ThemeTokens::builtin_dark(), Density::Roomy),
    );
    assert!(p.cell(&CellKind::Deck(1)).is_none());
    assert!(p.cell(&CellKind::Slot(head_slot(1))).is_none());
    assert!(p.cell(&CellKind::Deck(0)).is_some());
}

// ---- The generic grid --------------------------------------------------------------------------

const TWO_ROWS: &str = r#"{"rows":[{"height":0.66,"cells":[{"deck":0},{"slot":"mixer","weight":0.4},{"deck":1}]},{"height":0.34,"minHeight":150,"cells":[{"slot":"library"}]}]}"#;

#[test]
fn cells_tile_the_area_without_overlap() {
    let spec = LayoutSpec::from_json(TWO_ROWS).expect("layout");
    let area = Rect::new(10, 20, 1000, 700);
    let placed = compute_layout(&spec, area, 8);
    assert_eq!(placed.len(), 4);
    for (i, a) in placed.iter().enumerate() {
        assert!(area.contains(&a.bounds));
        assert!(!a.bounds.is_empty());
        for b in &placed[i + 1..] {
            assert!(!a.bounds.intersects(&b.bounds));
        }
    }
    let last = &placed[3];
    assert_eq!(
        last.bounds.right(),
        area.right(),
        "the last cell absorbs rounding"
    );
    assert_eq!(last.bounds.bottom(), area.bottom());
    assert!(
        placed[1].bounds.w < placed[0].bounds.w,
        "mixer weight is smaller"
    );
}

#[test]
fn nested_rows_parse_round_trip_and_reject_bad_input() {
    let json = r#"{"rows":[{"cells":[{"rows":[{"height":2,"cells":[{"slot":"a"}]},{"cells":[{"deck":0},{"slot":"b"}]}]}]}]}"#;
    let spec = LayoutSpec::from_json(json).expect("nested layout");
    assert_eq!(spec.deck_count(), 1);
    assert_eq!(spec.slot_names(), ["a", "b"]);
    assert_eq!(
        LayoutSpec::from_json(&spec.to_json()).expect("round trip"),
        spec
    );
    assert_eq!(compute_layout(&spec, Rect::sized(100, 90), 0).len(), 3);
    for bad in [
        r#"{"rows":[{"cells":[{"rows":[]}]}]}"#,
        r#"{"rows":[{"cells":[{"rows":[{"cells":[]}]}]}]}"#,
        r#"{"rows":[{"cells":[{"rows":[{"cells":[{"deck":0}]}],"deck":1}]}]}"#,
    ] {
        assert!(LayoutSpec::from_json(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn json_round_trips_and_bad_input_is_rejected() {
    let spec = LayoutSpec::from_json(TWO_ROWS).expect("layout");
    let parsed = LayoutSpec::from_json(&spec.to_json()).expect("round trip");
    assert_eq!(parsed, spec);
    assert!(spec.to_json().contains("minHeight"));
    for bad in [
        "",
        "{}",
        r#"{"rows":[]}"#,
        r#"{"rows":[{"cells":[]}]}"#,
        r#"{"rows":[{"cells":[{}]}]}"#,
        r#"{"rows":[{"cells":[{"deck":0,"slot":"x"}]}]}"#,
        r#"{"rows":[{"cells":[{"deck":0,"weight":0}]}]}"#,
        r#"{"rows":[{"height":-1,"cells":[{"deck":0}]}]}"#,
        r#"{"rows":[{"minHeight":-5,"cells":[{"deck":0}]}]}"#,
        "not json",
    ] {
        assert!(LayoutSpec::from_json(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn an_empty_spec_places_nothing_and_missing_decks_are_skipped() {
    assert!(compute_layout(&LayoutSpec::default(), Rect::sized(100, 100), 4).is_empty());
    let spec = LayoutSpec::from_json(TWO_ROWS).expect("layout");
    let shown = visible_cells(&spec, 1, Rect::sized(1600, 1000), 8);
    assert_eq!(shown.iter().filter(|c| c.kind.is_deck()).count(), 1);
}
