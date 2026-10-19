//! Main layout cases, plus the MainView placement rules.

use om_ui::geometry::Rect;
use om_ui::main_layout::{
    compute_layout, visible_cells, CellKind, LayoutSpec, LIBRARY_SLOT, MIXER_SLOT,
};

#[test]
fn standard_two_deck_layout_is_decks_around_mixer_then_library() {
    let spec = LayoutSpec::standard(2);
    assert_eq!(spec.rows.len(), 2);
    assert_eq!(spec.deck_count(), 2);
    assert_eq!(spec.slot_names(), [MIXER_SLOT, LIBRARY_SLOT]);
    let top = &spec.rows[0].cells;
    assert!(top[0].is_deck() && !top[1].is_deck() && top[2].is_deck());
}

#[test]
fn four_decks_add_a_row_and_keep_the_slots() {
    let spec = LayoutSpec::standard(4);
    assert_eq!(spec.deck_count(), 4);
    assert_eq!(spec.rows.len(), 3);
    assert_eq!(spec.slot_names().len(), 2);
}

#[test]
fn cells_tile_the_area_without_overlap() {
    let area = Rect::new(10, 20, 1000, 700);
    let placed = compute_layout(&LayoutSpec::standard(2), area, 8);
    assert_eq!(placed.len(), 4);
    for (i, a) in placed.iter().enumerate() {
        assert!(area.contains(&a.bounds));
        assert!(!a.bounds.is_empty());
        for b in &placed[i + 1..] {
            assert!(!a.bounds.intersects(&b.bounds));
        }
    }
    assert_eq!(
        placed[3].bounds.right(),
        area.right(),
        "the last cell absorbs rounding"
    );
    assert_eq!(placed[3].bounds.bottom(), area.bottom());
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
    assert!(d0.right() < mixer.x);
    assert!(mixer.right() < d1.x);
    assert!(library.y > d0.bottom());
    assert!(!d0.intersects(&mixer) && !d1.intersects(&mixer) && !mixer.intersects(&library));
}
