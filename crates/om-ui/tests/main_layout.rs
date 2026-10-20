//! Main layout cases, plus the MainView placement rules.

use om_ui::geometry::Rect;
use om_ui::main_layout::{
    compute_layout, visible_cells, CellKind, LayoutSpec, LIBRARY_SLOT, MIXER_SLOT, SAMPLER_SLOT,
};

#[test]
fn standard_two_deck_layout_is_decks_around_mixer_then_library() {
    let spec = LayoutSpec::standard(2);
    assert_eq!(spec.rows.len(), 2);
    assert_eq!(spec.deck_count(), 2);
    assert_eq!(spec.slot_names(), [MIXER_SLOT, LIBRARY_SLOT, SAMPLER_SLOT]);
    let top = &spec.rows[0].cells;
    assert!(top[0].is_deck() && !top[1].is_deck() && top[2].is_deck());
}

#[test]
fn four_decks_add_a_row_and_keep_the_slots() {
    let spec = LayoutSpec::standard(4);
    assert_eq!(spec.deck_count(), 4);
    assert_eq!(spec.rows.len(), 3);
    assert_eq!(spec.slot_names().len(), 3);
}

#[test]
fn cells_tile_the_area_without_overlap() {
    let area = Rect::new(10, 20, 1000, 700);
    let placed = compute_layout(&LayoutSpec::standard(2), area, 8);
    assert_eq!(placed.len(), 5);
    for (i, a) in placed.iter().enumerate() {
        assert!(area.contains(&a.bounds));
        assert!(!a.bounds.is_empty());
        for b in &placed[i + 1..] {
            assert!(!a.bounds.intersects(&b.bounds));
        }
    }
    assert_eq!(
        placed[4].bounds.right(),
        area.right(),
        "the last cell absorbs rounding"
    );
    assert_eq!(placed[4].bounds.bottom(), area.bottom());
    assert!(placed[1].bounds.w > 0);
    assert!(
        placed[1].bounds.w < placed[0].bounds.w,
        "mixer weight is smaller than a deck's"
    );
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

/// Window sizes the app must lay out cleanly: the common ones and the 900x600 minimum.
const WINDOWS: [(i32, i32); 4] = [(1280, 800), (1024, 640), (1920, 1080), (900, 600)];

#[test]
fn library_band_is_below_decks_and_mixer_at_every_window_size() {
    use om_ui::theme::ThemeTokens;
    use om_ui::widgets::deck::deck_layout;
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
            // Every part of a deck card (header, waveforms, pads, loops, transport) is inside the card.
            if a.kind.is_deck() {
                let l = deck_layout(Rect::sized(a.bounds.w, a.bounds.h), &t);
                let card = Rect::sized(a.bounds.w, a.bounds.h);
                let mut parts = vec![
                    l.header,
                    l.overview,
                    l.pitch,
                    l.hotcues,
                    l.loops,
                    l.transport,
                ];
                if l.zoom_visible {
                    parts.push(l.zoom);
                }
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

/// The deck card parts at the four window sizes the screenshots use. The main area is the window
/// less a 48 px header; each deck card is the cell `compute_layout` gives it. Every part sits inside
/// its card, no two parts overlap, and the stems row is present.
#[test]
fn deck_card_parts_fit_without_overlap_at_every_window_size() {
    use om_ui::geometry::Rect as R;
    use om_ui::main_layout::CellKind;
    use om_ui::widgets::deck::deck_layout;
    let theme = om_ui::theme::ThemeTokens::builtin_dark();
    for (width, height) in [(900, 600), (1280, 800), (1024, 640), (1920, 1080)] {
        let area = R::new(0, 48, width, height - 48);
        let spec = LayoutSpec::standard(2);
        for cell in compute_layout(&spec, area, 8) {
            let CellKind::Deck(_) = cell.kind else {
                continue;
            };
            let card = cell.bounds;
            let l = deck_layout(R::sized(card.w, card.h), &theme);
            assert!(l.stems.h > 0, "{width}x{height}: no stems row");
            assert!(
                l.zoom_visible,
                "{width}x{height}: the zoomed waveform is hidden (zoom {:?})",
                l.zoom
            );
            let mut parts = vec![
                l.header,
                l.overview,
                l.pitch,
                l.hotcues,
                l.loops,
                l.transport,
                l.stems,
            ];
            if l.zoom_visible {
                parts.push(l.zoom);
            }
            for part in &parts {
                assert!(
                    R::sized(card.w, card.h).contains(part),
                    "{width}x{height}: {part:?} leaves the card"
                );
            }
            for (i, a) in parts.iter().enumerate() {
                for b in &parts[i + 1..] {
                    assert!(!a.intersects(b), "{width}x{height}: {a:?} overlaps {b:?}");
                }
            }
        }
    }
}

/// The sampler strip is at least 250 px wide and 100 px tall at the four sizes and the 900x600
/// minimum, so its pads and controls stay usable.
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
