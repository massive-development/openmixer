//! Main layout cases, plus the MainView placement rules.

use om_ui::geometry::Rect;
use om_ui::main_layout::{
    channel_of_slot, channel_slot, compute_layout, visible_cells, CellKind, LayoutSpec,
    LIBRARY_SLOT, MIXER_SLOT, SAMPLER_SLOT, STAGE_SLOT,
};

#[test]
fn standard_two_deck_layout_is_strips_beside_the_stage_and_controls_then_the_library() {
    let spec = LayoutSpec::standard(2);
    assert_eq!(spec.rows.len(), 2);
    assert_eq!(spec.deck_count(), 2);
    assert_eq!(
        spec.slot_names(),
        [
            channel_slot(0).as_str(),
            STAGE_SLOT,
            MIXER_SLOT,
            channel_slot(1).as_str(),
            LIBRARY_SLOT,
            SAMPLER_SLOT
        ]
    );
    let top = &spec.rows[0].cells;
    assert_eq!(top.len(), 3, "strip, centre column, strip");
    assert!(matches!(top[1].kind, CellKind::Rows(_)));
}

#[test]
fn four_decks_add_a_row_and_keep_the_slots() {
    let spec = LayoutSpec::standard(4);
    assert_eq!(spec.deck_count(), 4);
    assert_eq!(spec.rows.len(), 3);
    // Stage, mixer module, library, sampler and one strip per deck.
    assert_eq!(spec.slot_names().len(), 8);
}

#[test]
fn channel_slot_names_round_trip() {
    assert_eq!(channel_of_slot(&channel_slot(3)), Some(3));
    assert_eq!(channel_of_slot("mixer"), None);
    assert_eq!(channel_of_slot("channel:x"), None);
}

#[test]
fn cells_tile_the_area_without_overlap() {
    let area = Rect::new(10, 20, 1000, 700);
    let placed = compute_layout(&LayoutSpec::standard(2), area, 8);
    // Strip, stage, deck 0, mixer, deck 1, strip, library, sampler: groups are flattened.
    assert_eq!(placed.len(), 8);
    assert!(placed.iter().all(|p| !matches!(p.kind, CellKind::Rows(_))));
    for (i, a) in placed.iter().enumerate() {
        assert!(area.contains(&a.bounds));
        assert!(!a.bounds.is_empty());
        for b in &placed[i + 1..] {
            assert!(!a.bounds.intersects(&b.bounds));
        }
    }
    let last = &placed[7];
    assert_eq!(
        last.bounds.right(),
        area.right(),
        "the last cell absorbs rounding"
    );
    assert_eq!(last.bounds.bottom(), area.bottom());
    let (deck0, mixer) = (&placed[2], &placed[3]);
    assert!(mixer.bounds.w > 0);
    assert!(
        mixer.bounds.w < deck0.bounds.w,
        "mixer weight is smaller than a deck's"
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
    let placed = compute_layout(&spec, Rect::sized(100, 90), 0);
    assert_eq!(placed.len(), 3);
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
    let spec = LayoutSpec::standard(4);
    let parsed = LayoutSpec::from_json(&spec.to_json()).expect("round trip");
    assert_eq!(parsed.deck_count(), 4);
    assert_eq!(parsed.rows.len(), spec.rows.len());
    assert_eq!(parsed.to_json(), spec.to_json());
    for bad in [
        "",
        "{}",
        r#"{"rows":[]}"#,
        r#"{"rows":[{"cells":[]}]}"#,
        r#"{"rows":[{"cells":[{}]}]}"#,
        r#"{"rows":[{"cells":[{"deck":0,"slot":"x"}]}]}"#,
        r#"{"rows":[{"cells":[{"deck":0,"weight":0}]}]}"#,
        r#"{"rows":[{"height":-1,"cells":[{"deck":0}]}]}"#,
        "not json",
    ] {
        assert!(LayoutSpec::from_json(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn hand_written_json_is_accepted() {
    let spec = LayoutSpec::from_json(
        r#"{"rows":[{"height":1,"cells":[{"deck":0},{"slot":"mixer","weight":0.5},{"deck":1},{"slot":"plugin.x"}]}]}"#,
    )
    .expect("hand-written layout");
    assert_eq!(spec.slot_names(), ["mixer", "plugin.x"]);
}

#[test]
fn an_empty_spec_places_nothing() {
    assert!(compute_layout(&LayoutSpec::default(), Rect::sized(100, 100), 4).is_empty());
}

#[test]
fn decks_the_engine_lacks_are_not_placed() {
    let area = Rect::sized(1600, 1000);
    let shown = visible_cells(&LayoutSpec::standard(4), 2, area, 8);
    assert!(shown
        .iter()
        .all(|c| !matches!(c.kind, CellKind::Deck(d) if d >= 2)));
    assert_eq!(shown.iter().filter(|c| c.kind.is_deck()).count(), 2);
}

#[test]
fn mixer_sits_between_the_decks_and_the_library_is_below() {
    let area = Rect::sized(1280, 800);
    let placed = visible_cells(&LayoutSpec::standard(2), 2, area, 12);
    let find = |kind: CellKind| placed.iter().find(|c| c.kind == kind).expect("cell").bounds;
    let d0 = find(CellKind::Deck(0));
    let d1 = find(CellKind::Deck(1));
    let mixer = find(CellKind::Slot(MIXER_SLOT.into()));
    let library = find(CellKind::Slot(LIBRARY_SLOT.into()));
    let sampler = find(CellKind::Slot(SAMPLER_SLOT.into()));
    assert!(d0.right() < mixer.x);
    assert!(mixer.right() < d1.x);
    assert!(library.y > d0.bottom());
    assert_eq!(
        sampler.y, library.y,
        "the sampler strip shares the library band"
    );
    assert!(
        library.right() < sampler.x,
        "the strip sits beside the library"
    );
    assert!(!d0.intersects(&mixer) && !d1.intersects(&mixer) && !mixer.intersects(&library));
    assert!(!sampler.intersects(&library) && !sampler.intersects(&mixer));
}

/// Window sizes the app must lay out cleanly: the common ones and the 1024x640 minimum
/// (`MIN_WINDOW`; the window cannot be resized below it).
const WINDOWS: [(i32, i32); 3] = [(1280, 800), (1024, 640), (1920, 1080)];

#[test]
fn library_band_is_below_decks_and_mixer_at_every_window_size() {
    use om_ui::theme::ThemeTokens;
    use om_ui::widgets::deck::deck_controls_layout;
    let t = ThemeTokens::builtin_dark();
    for (w, h) in WINDOWS {
        // Banner (24) and header strip above the layout, as the app has them.
        let top = 24 + 36;
        let window = Rect::sized(w, h);
        let area = Rect::new(0, top, w, h - top);
        let placed = compute_layout(&LayoutSpec::standard(2), area, t.gap);
        let library = placed
            .iter()
            .find(|p| p.kind == CellKind::Slot(LIBRARY_SLOT.to_owned()))
            .expect("library cell");
        for (i, a) in placed.iter().enumerate() {
            assert!(window.contains(&a.bounds), "{w}x{h}: {a:?} outside window");
            for b in &placed[i + 1..] {
                assert!(
                    !a.bounds.intersects(&b.bounds),
                    "{w}x{h}: {a:?} overlaps {b:?}"
                );
            }
            // The library and the sampler strip share the bottom band; everything else sits above it.
            let in_band =
                |k: &CellKind| *k == library.kind || *k == CellKind::Slot(SAMPLER_SLOT.to_owned());
            if !in_band(&a.kind) {
                assert!(
                    a.bounds.bottom() <= library.bounds.y,
                    "{w}x{h}: {:?} reaches into the library band",
                    a.kind
                );
            }
            // Every control of a deck card (pads, loops, transport, stems) is inside the card.
            if a.kind.is_deck() {
                let card = Rect::sized(a.bounds.w, a.bounds.h);
                let l = deck_controls_layout(card, &t);
                let parts = [l.pitch, l.hotcues, l.loops, l.transport, l.tabs, l.stems];
                for (i, p) in parts.iter().enumerate() {
                    assert!(
                        card.contains(p),
                        "{w}x{h}: deck part {i} {p:?} outside card"
                    );
                    for q in &parts[i + 1..] {
                        assert!(
                            !p.intersects(q),
                            "{w}x{h}: deck parts {p:?} and {q:?} overlap"
                        );
                    }
                }
            }
        }
    }
}

/// The deck controls and the waveform stage at the four window sizes the screenshots use. The main
/// area is the window less a 48 px header. Every part sits inside its cell and no two parts overlap;
/// the stems row is present and each zoomed waveform is tall enough to read.
#[test]
fn deck_controls_and_stage_fit_without_overlap_at_every_window_size() {
    use om_ui::geometry::Rect as R;
    use om_ui::widgets::deck::deck_controls_layout;
    use om_ui::widgets::stage::stage_layout;
    let theme = om_ui::theme::ThemeTokens::builtin_dark();
    for (width, height) in [(1280, 800), (1024, 640), (1920, 1080)] {
        let area = R::new(0, 48, width, height - 48);
        let spec = LayoutSpec::standard(2);
        for cell in compute_layout(&spec, area, 8) {
            let card = R::sized(cell.bounds.w, cell.bounds.h);
            let mut parts = Vec::new();
            match &cell.kind {
                CellKind::Deck(_) => {
                    let l = deck_controls_layout(card, &theme);
                    assert!(l.tabs.h >= 32, "{width}x{height}: no tab strip");
                    parts = vec![l.pitch, l.hotcues, l.loops, l.transport, l.tabs];
                }
                CellKind::Slot(name) if name == STAGE_SLOT => {
                    let layouts = stage_layout(card, &theme, 2);
                    assert_eq!(layouts.len(), 2);
                    for l in &layouts {
                        assert!(
                            l.zoom_visible,
                            "{width}x{height}: the zoomed waveform is hidden (zoom {:?})",
                            l.zoom
                        );
                        parts.extend([l.header, l.overview, l.zoom]);
                    }
                }
                CellKind::Slot(_) | CellKind::Rows(_) => {}
            }
            for part in &parts {
                assert!(
                    card.contains(part),
                    "{width}x{height}: {:?} {part:?} leaves its cell",
                    cell.kind
                );
            }
            for (i, a) in parts.iter().enumerate() {
                for b in &parts[i + 1..] {
                    assert!(
                        !a.intersects(b),
                        "{width}x{height}: {:?} {a:?} overlaps {b:?}",
                        cell.kind
                    );
                }
            }
        }
    }
}

/// The sampler strip is at least 250 px wide and 100 px tall at the three sizes (the smallest is the
/// minimum window), so its pads and controls stay usable.
#[test]
fn sampler_strip_has_room_in_the_library_band() {
    for (w, h) in WINDOWS {
        let area = Rect::new(0, 60, w, h - 60);
        let placed = compute_layout(&LayoutSpec::standard(2), area, 8);
        let strip = placed
            .iter()
            .find(|p| p.kind == CellKind::Slot(SAMPLER_SLOT.to_owned()))
            .expect("sampler cell");
        assert!(strip.bounds.w >= 250, "{w}x{h}: strip {:?}", strip.bounds);
        assert!(strip.bounds.h >= 100, "{w}x{h}: strip {:?}", strip.bounds);
    }
}

/// The automix bar shares the header strip above the layout: at every window size it fits in the
/// strip (left of the 96 px Settings button), inside the window, and above the cells, so the deck
/// rows keep the room they had before the bar existed.
#[test]
fn automix_bar_fits_in_the_header_strip_at_every_size() {
    use om_ui::automix_bar::BAR_HEIGHT;
    let t = om_ui::theme::ThemeTokens::builtin_dark();
    let (banner, header) = (24, 36);
    for (w, h) in WINDOWS {
        let strip = Rect::new(0, banner, w - 96, BAR_HEIGHT);
        let window = Rect::sized(w, h);
        assert!(
            window.contains(&strip),
            "{w}x{h}: bar {strip:?} outside window"
        );
        assert!(
            strip.bottom() <= banner + header,
            "{w}x{h}: bar leaves the header"
        );
        for cell in compute_layout(
            &LayoutSpec::standard(2),
            Rect::new(0, banner + header, w, h - banner - header),
            t.gap,
        ) {
            assert!(
                !cell.bounds.intersects(&strip),
                "{w}x{h}: {:?} under the bar",
                cell.kind
            );
        }
    }
}

#[test]
fn a_row_floor_holds_on_a_short_window_and_the_other_rows_give_way() {
    use om_ui::main_layout::{CONTROLS_MIN_HEIGHT, MIN_WINDOW};
    let spec = LayoutSpec::standard(2);
    let area = Rect::new(0, 60, MIN_WINDOW.0, MIN_WINDOW.1 - 60);
    let placed = compute_layout(&spec, area, 12);
    let find = |kind: CellKind| placed.iter().find(|c| c.kind == kind).expect("cell").bounds;
    assert!(find(CellKind::Deck(0)).h >= CONTROLS_MIN_HEIGHT);
    let tall = compute_layout(&spec, Rect::new(0, 60, 1920, 1020), 12);
    let stage = |cells: &[om_ui::main_layout::PlacedCell]| {
        cells
            .iter()
            .find(|c| c.kind == CellKind::Slot(STAGE_SLOT.into()))
            .expect("stage")
            .bounds
            .h
    };
    assert!(
        stage(&tall) > stage(&placed),
        "the stage takes the spare height"
    );
}

#[test]
fn min_height_round_trips_through_json_and_rejects_nonsense() {
    let spec = LayoutSpec::standard(2);
    assert!(spec.to_json().contains("minHeight"));
    assert_eq!(
        LayoutSpec::from_json(&spec.to_json()).expect("round trip"),
        spec
    );
    assert!(LayoutSpec::from_json(r#"{"rows":[{"minHeight":-5,"cells":[{"deck":0}]}]}"#).is_err());
}
