//! The library browser's window glue, driven without a window: rows and tint, the search keys,
//! the sidebar and sorting, the load buttons and the deck drop, all through [`LibraryPanel`] over an
//! in-memory library and real WAV files. Nothing here opens a device: the engine's audio side is
//! dropped, so the decks load and report, but never play.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use om_engine::DjEngine;
use om_library::TrackRecord;
use om_ui::library::{describe_track_drag, LibraryServices, Options};
use om_ui::sidebar::SidebarKind;
use om_ui_gpui::library::{KeyOutcome, LibraryHub, LibraryPanel, SharedHub};

/// A 16-bit mono WAV with a short tone, enough for the loader to decode.
fn write_wav(path: &Path) {
    const RATE: u32 = 8_000;
    let frames: u32 = RATE / 4;
    let data_len = frames * 2;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for n in 0..frames {
        let sample = ((n as f32 * 0.3).sin() * 4000.0) as i16;
        out.extend_from_slice(&sample.to_le_bytes());
    }
    fs::File::create(path).unwrap().write_all(&out).unwrap();
}

struct Fixture {
    dir: PathBuf,
    hub: SharedHub,
}

fn memory_services() -> LibraryServices {
    LibraryServices::create(Options::for_path(":memory:")).expect("in-memory library opens")
}

fn fixture(name: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("om-library-panel-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let services = memory_services();
    let hub = LibraryHub::shared(services, 2);
    Fixture { dir, hub }
}

/// Inserts a track whose file exists (a real WAV), or a missing one when `real` is false.
fn add(fx: &Fixture, title: &str, camelot: &str, real: bool) -> i64 {
    let path = fx.dir.join(format!("{title}.wav"));
    if real {
        write_wav(&path);
    }
    let track = TrackRecord {
        path: path.to_string_lossy().into_owned(),
        title: title.to_owned(),
        artist: "Tester".to_owned(),
        camelot: camelot.to_owned(),
        bpm: Some(124.0),
        duration_sec: 0.25,
        ..TrackRecord::default()
    };
    fx.hub
        .borrow()
        .services
        .library()
        .tracks()
        .insert(&track)
        .unwrap()
}

fn panel(fx: &Fixture) -> (LibraryPanel, Arc<DjEngine>) {
    let (engine, _audio) = DjEngine::new(2);
    let engine = Arc::new(engine);
    let panel = LibraryPanel::new(Rc::clone(&fx.hub), Arc::clone(&engine));
    (panel, engine)
}

fn type_text(panel: &mut LibraryPanel, text: &str) {
    for c in text.chars() {
        let key = c.to_string();
        panel.handle_key(&key, Some(&key));
    }
}

#[test]
fn three_tracks_show_three_rows_and_the_first_loads_into_deck_a() {
    let fx = fixture("rows");
    add(&fx, "Alpha", "8A", true);
    add(&fx, "Bravo", "9A", true);
    add(&fx, "Charlie", "3B", true);
    let (mut panel, engine) = panel(&fx);
    panel.tick();
    assert_eq!(panel.row_count(), 3);
    let first = panel.row(0).expect("row 0");
    assert_eq!(first.cells.len(), om_ui::library::columns().len());
    assert!(first.drag.starts_with("openmixer.track:"));

    panel.select_row(0);
    panel.load_selected(0);
    assert!(panel.has_track(0));
    assert!(engine.deck(0).is_loaded());
    assert!(!panel.browser.status_is_error());
}

#[test]
fn double_click_and_enter_load_into_the_suggested_idle_deck() {
    let fx = fixture("activate");
    add(&fx, "Alpha", "8A", true);
    add(&fx, "Bravo", "9A", true);
    let (mut panel, _engine) = panel(&fx);
    panel.tick();

    // Deck A is empty, so it is the suggestion.
    assert!(panel.activate_row(1).expect("row exists").is_ok());
    assert!(panel.has_track(0));
    assert!(!panel.has_track(1));

    // Enter loads the selected row into the next idle deck (B, still empty).
    panel.select_row(0);
    assert_eq!(panel.handle_key("enter", None), KeyOutcome::Activated);
    assert!(panel.has_track(1));
}

#[test]
fn dropping_a_library_row_on_a_deck_loads_that_deck_only() {
    let fx = fixture("drop");
    let id = add(&fx, "Alpha", "8A", true);
    let (mut panel, engine) = panel(&fx);
    panel.tick();
    let description = panel.row(0).unwrap().drag;
    assert_eq!(description, describe_track_drag(id));

    assert!(panel
        .drop_on_deck(1, &description)
        .expect("a library drag")
        .is_ok());
    assert!(panel.has_track(1));
    assert!(!panel.has_track(0));
    assert!(engine.deck(1).is_loaded());

    // Anything that is not a library track is not handled here.
    assert!(panel.drop_on_deck(0, "some other drag").is_none());
}

#[test]
fn a_missing_file_is_reported_in_the_status_and_the_deck_stays_empty() {
    let fx = fixture("missing");
    let id = add(&fx, "Ghost", "8A", false);
    let (mut panel, engine) = panel(&fx);
    panel.tick();
    let result = panel.load_track_id(0, id);
    assert!(result.is_err());
    assert!(panel.browser.status_is_error());
    assert!(panel.browser.status().contains("Ghost"));
    assert!(!panel.has_track(0));
    assert!(!engine.deck(0).is_loaded());
}

#[test]
fn search_keys_edit_the_query_and_apply_on_enter() {
    let fx = fixture("search");
    add(&fx, "House Anthem", "8A", true);
    add(&fx, "Techno Pulse", "9A", true);
    add(&fx, "Deep House", "3B", true);
    let (mut panel, _engine) = panel(&fx);
    panel.tick();
    assert_eq!(panel.row_count(), 3);

    assert_eq!(panel.handle_key("/", Some("/")), KeyOutcome::Edited);
    assert!(panel.search_active);
    type_text(&mut panel, "house");
    panel.tick();
    assert_eq!(panel.row_count(), 2, "the query runs on the tick");

    assert_eq!(panel.handle_key("backspace", None), KeyOutcome::Edited);
    assert_eq!(panel.browser.search_text(), "hous");
    panel.handle_key("enter", None);
    assert!(!panel.search_active);
    assert_eq!(panel.browser.search_text(), "hous");

    // Escape leaves the box and clears the query.
    panel.handle_key("/", Some("/"));
    panel.handle_key("escape", None);
    assert!(!panel.search_active);
    assert_eq!(panel.browser.search_text(), "");
    assert_eq!(panel.row_count(), 3);
}

#[test]
fn arrow_keys_move_the_selection_and_keep_it_on_screen() {
    let fx = fixture("arrows");
    for (i, title) in ["A1", "A2", "A3", "A4"].iter().enumerate() {
        add(&fx, title, "", true);
        let _ = i;
    }
    let (mut panel, _engine) = panel(&fx);
    panel.tick();
    panel.visible_rows = 2;
    assert_eq!(panel.handle_key("down", None), KeyOutcome::Moved);
    assert_eq!(panel.browser.selected_row(), Some(0));
    for _ in 0..3 {
        panel.handle_key("down", None);
    }
    assert_eq!(panel.browser.selected_row(), Some(3));
    assert_eq!(
        panel.table_top, 2,
        "the selected row is the last visible one"
    );
    panel.handle_key("up", None);
    assert_eq!(panel.browser.selected_row(), Some(2));
    assert_eq!(panel.table_top, 2);
}

#[test]
fn scrolling_stays_inside_the_list() {
    let fx = fixture("scroll");
    for title in ["S1", "S2", "S3"] {
        add(&fx, title, "", true);
    }
    let (mut panel, _engine) = panel(&fx);
    panel.tick();
    panel.visible_rows = 2;
    panel.scroll_rows(5);
    assert_eq!(panel.table_top, 1);
    panel.scroll_rows(-9);
    assert_eq!(panel.table_top, 0);
}

#[test]
fn compatible_keys_are_tinted_against_the_loaded_reference() {
    let fx = fixture("tint");
    let reference = add(&fx, "Reference", "8A", true);
    add(&fx, "Adjacent", "9A", true);
    add(&fx, "Clash", "3B", true);
    add(&fx, "Unknown", "", true);
    let (mut panel, _engine) = panel(&fx);
    panel.tick();
    panel.load_track_id(0, reference).unwrap();
    panel.tick();

    let mut tints = Vec::new();
    for row in 0..panel.row_count() {
        let view = panel.row(row).unwrap();
        tints.push((view.cells[0].clone(), view.compatible));
    }
    let tint_of = |title: &str| {
        tints
            .iter()
            .find(|(cell, _)| cell.contains(title))
            .map(|(_, c)| *c)
            .unwrap_or(false)
    };
    assert!(tint_of("Adjacent"), "9A is one step from 8A on the wheel");
    assert!(!tint_of("Clash"), "3B clashes with 8A");
    assert!(!tint_of("Unknown"), "no key, no tint");
}

#[test]
fn sidebar_starts_on_all_tracks_and_sorting_flips_on_a_second_click() {
    let fx = fixture("sort");
    add(&fx, "Zulu", "", true);
    add(&fx, "Alpha", "", true);
    let (mut panel, _engine) = panel(&fx);
    panel.tick();

    let lines = panel.sidebar_lines();
    assert_eq!(lines[0].node.kind, SidebarKind::AllTracks);
    assert!(lines[0].selected);
    assert!(panel.select_sidebar(&lines[0].node));

    let title_column = om_ui::library::columns()
        .iter()
        .find(|c| c.sort_field == Some("title"))
        .expect("title is sortable")
        .id;
    assert!(panel.sort_by_column(title_column));
    assert_eq!(panel.sort_indicator(), Some((title_column, true)));
    assert!(panel.sort_by_column(title_column));
    assert_eq!(panel.sort_indicator(), Some((title_column, false)));
}
