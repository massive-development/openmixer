//! Port of tests/unit/LibraryBrowserTest.cpp (logic parts) and the LibraryServicesTest cases. The
//! view cases (glass theme snapshots, toolbar layout, search-box centring, deck drop targets) are
//! gpui concerns and are not ported here.

mod library_common;

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use library_common::*;
use om_library::{PrepStatus, TrackRecord};
use om_ui::library::{
    describe_track_drag, parse_track_drag, LibraryBrowser, LibraryEvent, LibraryServices, Options,
    TrackSource, TrackTableModel, PAGE_SIZE,
};
use om_ui::sidebar::{build_sidebar, SidebarKind};

const REKORDBOX_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/data/import/rekordbox_small.xml"
);

struct BrowserRig {
    services: LibraryServices,
    ids: Vec<i64>,
    browser: LibraryBrowser,
}

impl BrowserRig {
    fn new() -> Self {
        let mut services = make_services(memory_options());
        let ids = seed_four(services.library());
        let browser = LibraryBrowser::new(&mut services, 2);
        Self {
            services,
            ids,
            browser,
        }
    }

    fn title(&mut self, row: isize) -> String {
        self.browser.model.cell_text(row, 1)
    }

    fn cell(&mut self, row: isize, column: i32) -> String {
        self.browser.model.cell_text(row, column)
    }

    fn rows(&self) -> usize {
        self.browser.model.num_rows()
    }

    fn set_search(&mut self, text: &str) {
        self.browser.set_search_text(&self.services, text);
    }

    fn row(&mut self, row: isize) -> TrackRecord {
        self.browser.model.row_at(row).cloned().expect("row exists")
    }
}

#[test]
fn table_shows_seeded_tracks_with_formatted_cells() {
    let mut rig = BrowserRig::new();
    assert_eq!(rig.rows(), 4);
    assert_eq!(rig.cell(0, 1), "Alpha");
    assert_eq!(rig.cell(0, 2), "Zed");
    assert_eq!(rig.cell(0, 3), "120.0");
    assert_eq!(rig.cell(0, 4), "8A");
    assert_eq!(rig.cell(0, 5), "4:05");
    assert_eq!(rig.cell(0, 7), "House");
    assert_eq!(rig.cell(3, 3), "--", "not analysed yet");
    assert_eq!(rig.cell(0, 8).len(), 10, "yyyy-mm-dd");
    assert_eq!(rig.browser.status(), "4 tracks");
}

#[test]
fn search_box_uses_the_query_syntax() {
    let mut rig = BrowserRig::new();
    rig.set_search("genre:house");
    assert_eq!(rig.rows(), 2);
    rig.set_search("bpm:125-130");
    assert_eq!(rig.rows(), 1);
    assert_eq!(rig.title(0), "Bravo");
    rig.set_search("genre:house -artist:zed");
    assert_eq!(rig.rows(), 1);
    assert_eq!(rig.title(0), "Charlie");
    rig.set_search("key:~8A"); // 8A, 9A, 7A, 8B
    assert_eq!(rig.rows(), 2);
    rig.set_search("sort:-bpm genre:house");
    assert_eq!(rig.title(0), "Charlie");
    rig.set_search("");
    assert_eq!(rig.rows(), 4);
}

#[test]
fn query_problems_show_in_the_status_line() {
    let mut rig = BrowserRig::new();
    rig.set_search("sort:");
    assert!(
        rig.browser.status().contains("sort"),
        "{}",
        rig.browser.status()
    );
    assert!(rig.browser.status_is_error());
    rig.set_search("alpha");
    assert_eq!(rig.browser.status(), "1 tracks");
    assert!(!rig.browser.status_is_error());
}

#[test]
fn typing_in_the_search_box_is_applied_by_the_debounce_tick() {
    let mut rig = BrowserRig::new();
    rig.browser.type_search("bravo");
    assert_eq!(rig.rows(), 4, "not yet");
    rig.browser.flush_pending(&mut rig.services);
    assert_eq!(rig.rows(), 1);
}

#[test]
fn header_clicks_sort_through_the_search_engine() {
    let mut rig = BrowserRig::new();
    rig.browser.header_sort(&rig.services, 3, false); // BPM, descending
    assert_eq!(rig.title(0), "Bravo");
    assert_eq!(rig.title(1), "Charlie");
    rig.browser.header_sort(&rig.services, 3, true);
    assert_eq!(rig.title(3), "Bravo");
    rig.browser.header_sort(&rig.services, 2, true); // artist ascending: Wu, Xi, Yan, Zed
    assert_eq!(rig.title(0), "Delta");
    assert_eq!(rig.title(3), "Alpha");
    rig.browser.header_sort(&rig.services, 1, false); // title descending
    assert_eq!(rig.title(0), "Delta");
    rig.browser.header_sort(&rig.services, 6, true); // rating ascending
    assert_eq!(rig.title(0), "Delta");
    assert_eq!(rig.title(3), "Alpha");
    let search = rig.services.search();
    for column in om_ui::library::columns() {
        if let Some(field) = column.sort_field {
            assert!(
                search.registry().find(field).is_some(),
                "{field} is a registered search field"
            );
        }
    }
}

struct CountingSource {
    total: usize,
    fetches: Rc<Cell<usize>>,
}

impl TrackSource for CountingSource {
    fn count(&mut self) -> usize {
        self.total
    }

    fn fetch(&mut self, offset: usize, limit: usize) -> Vec<TrackRecord> {
        self.fetches.set(self.fetches.get() + 1);
        (offset..self.total.min(offset + limit))
            .map(|i| TrackRecord {
                id: i as i64 + 1,
                title: format!("T{i}"),
                ..TrackRecord::default()
            })
            .collect()
    }
}

#[test]
fn rows_are_paged_and_only_fetched_when_shown() {
    let fetches = Rc::new(Cell::new(0));
    let mut model = TrackTableModel::new();
    model.set_source(Box::new(CountingSource {
        total: 1000,
        fetches: Rc::clone(&fetches),
    }));
    assert_eq!(model.num_rows(), 1000);
    assert_eq!(fetches.get(), 0, "counting does not read rows");
    assert!(model.row_at(0).is_some());
    assert!(model.row_at(99).is_some());
    assert_eq!(fetches.get(), 1);
    assert!(model.row_at(100).is_some());
    assert_eq!(fetches.get(), 2);
    assert_eq!(
        model.row_at(999).map(|t| t.title.clone()).as_deref(),
        Some("T999")
    );
    assert_eq!(fetches.get(), 3);
    for page in 0..10 {
        model.row_at((page * PAGE_SIZE) as isize); // walking the list stays bounded by the cache
    }
    assert!(model.row_at(1000).is_none());
    assert!(model.row_at(-1).is_none());
    let before = fetches.get();
    model.row_at(950); // the recently used page is still cached
    assert_eq!(fetches.get(), before);
}

#[test]
fn harmonically_compatible_keys_are_flagged_against_the_reference() {
    let mut rig = BrowserRig::new();
    let (a, b, c, d) = (rig.row(0), rig.row(1), rig.row(2), rig.row(3));
    assert!(!rig.browser.model.is_compatible(&a), "no reference yet");
    rig.browser.model.set_reference_camelot("8A");
    assert!(rig.browser.model.is_compatible(&a), "same key");
    assert!(
        rig.browser.model.is_compatible(&b),
        "9A, one step on the wheel"
    );
    assert!(!rig.browser.model.is_compatible(&c), "3B");
    assert!(!rig.browser.model.is_compatible(&d), "unknown key");
    rig.browser.model.set_reference_camelot("");
    assert!(!rig.browser.model.is_compatible(&a));
}

#[test]
fn reference_key_provider_is_polled_by_the_tick() {
    let mut rig = BrowserRig::new();
    let camelot = Rc::new(RefCell::new(String::from("9A")));
    let source = Rc::clone(&camelot);
    rig.browser.reference_key = Some(Box::new(move || source.borrow().clone()));
    rig.browser.flush_pending(&mut rig.services);
    let alpha = rig.row(0);
    assert!(rig.browser.model.is_compatible(&alpha), "8A next to 9A");
    *camelot.borrow_mut() = "3B".to_owned();
    rig.browser.flush_pending(&mut rig.services);
    assert!(!rig.browser.model.is_compatible(&alpha));
}

#[test]
fn sidebar_lists_all_tracks_playlists_watch_folders_and_the_prep_queue() {
    let mut rig = BrowserRig::new();
    let lib = rig.services.library();
    let sets = lib.playlists().create("Sets", None, true).unwrap();
    let warmup = lib.playlists().create("Warmup", Some(sets), false).unwrap();
    let peak = lib.playlists().create("Peak", None, false).unwrap();
    lib.playlists().add_track(peak, rig.ids[1], -1).unwrap();
    lib.playlists().add_track(warmup, rig.ids[0], -1).unwrap();
    lib.watch_folders().add("/music/house").unwrap();
    rig.services.notify_library_changed();
    rig.browser.flush_pending(&mut rig.services);

    let nodes = rig.browser.sidebar_nodes().to_vec();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].kind, SidebarKind::AllTracks);
    assert_eq!(nodes[1].label, "Playlists");
    assert_eq!(nodes[1].children.len(), 2);
    assert_eq!(nodes[1].children[0].label, "Sets");
    assert_eq!(nodes[1].children[0].kind, SidebarKind::Folder);
    assert_eq!(nodes[1].children[0].children.len(), 1);
    assert_eq!(nodes[1].children[0].children[0].label, "Warmup");
    assert_eq!(nodes[1].children[1].label, "Peak");
    assert_eq!(nodes[2].children.len(), 1);
    assert_eq!(nodes[2].children[0].path, "/music/house");
    assert_eq!(nodes[2].children[0].label, "house");
    assert_eq!(nodes[3].kind, SidebarKind::PrepQueue);
    assert!(!nodes[1].is_selectable());
    assert_eq!(
        nodes[1].children[1].scope_query(),
        format!("playlist:#{peak}")
    );

    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::Playlist, peak, ""));
    assert_eq!(rig.rows(), 1);
    assert_eq!(rig.title(0), "Bravo");
    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::Playlist, warmup, ""));
    assert_eq!(rig.title(0), "Alpha");
    rig.set_search("alpha"); // search applies inside the selected playlist
    assert_eq!(rig.rows(), 1);
    rig.set_search("bravo");
    assert_eq!(rig.rows(), 0);
    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::AllTracks, 0, ""));
    assert_eq!(rig.rows(), 1);
}

#[test]
fn watch_folder_node_scopes_the_list_to_that_path() {
    let mut rig = BrowserRig::new();
    add_track(
        rig.services.library(),
        &Seed {
            title: "InFolder",
            artist: "A",
            bpm: Some(100.0),
            camelot: "1A",
            genre: "",
            rating: 0,
        },
        Some("/music/house/in.wav"),
    );
    rig.services
        .library()
        .watch_folders()
        .add("/music/house")
        .unwrap();
    rig.browser.refresh(&rig.services);
    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::WatchFolder, 0, "/music/house"));
    assert_eq!(rig.rows(), 1);
    assert_eq!(rig.title(0), "InFolder");
}

#[test]
fn prep_queue_node_lists_queued_tracks_and_prep_button_queues_selection() {
    let mut rig = BrowserRig::new();
    let dir = tempfile::tempdir().unwrap();
    for name in ["Real1", "Real2"] {
        let wav = write_wav(&dir.path().join(format!("{name}.wav")), 1.0, 220.0);
        add_track(
            rig.services.library(),
            &Seed {
                title: name,
                artist: "Real",
                bpm: None,
                camelot: "",
                genre: "",
                rating: 0,
            },
            Some(&wav.to_string_lossy()),
        );
    }
    rig.set_search("artist:real");
    rig.browser.select_range(0, 1);
    assert_eq!(rig.browser.prep_selected(&mut rig.services), 2);
    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::PrepQueue, 0, ""));
    assert_eq!(rig.rows(), 2);
    assert_eq!(rig.browser.status(), "2 in the prep queue");
    assert!(pump_until(&mut rig.services, |s| {
        s.prep_queue()
            .list()
            .map(|list| list.iter().all(|e| e.status == PrepStatus::Done))
            .unwrap_or(false)
    }));
    rig.browser.flush_pending(&mut rig.services);
    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::AllTracks, 0, ""));
    rig.set_search("artist:real");
    assert_eq!(
        rig.cell(0, 3),
        "128.0",
        "analysis wrote bpm back and the table shows it"
    );
    assert_eq!(rig.cell(0, 4), "8A");
}

#[test]
fn load_buttons_and_activation_request_the_right_deck() {
    let mut rig = BrowserRig::new();
    let loads: Rc<RefCell<Vec<(usize, String)>>> = Rc::default();
    let sink = Rc::clone(&loads);
    rig.browser.on_load_requested = Some(Box::new(move |deck, t: &TrackRecord| {
        sink.borrow_mut().push((deck, t.title.clone()));
        Ok(())
    }));
    rig.browser.default_deck = Some(Box::new(|| 1));

    rig.browser.click_load(0); // nothing selected: no load
    assert!(loads.borrow().is_empty());
    rig.browser.select_row(1);
    rig.browser.click_load(1);
    rig.browser.click_load(0);
    assert_eq!(
        *loads.borrow(),
        vec![(1, "Bravo".to_owned()), (0, "Bravo".to_owned())]
    );

    rig.browser
        .activate_row(2)
        .expect("row exists")
        .expect("loads");
    assert_eq!(loads.borrow()[2], (1, "Charlie".to_owned()));
}

#[test]
fn failed_loads_show_the_reason_in_the_status_line() {
    let mut rig = BrowserRig::new();
    rig.browser.on_load_requested = Some(Box::new(|_, _| Err("file is missing".to_owned())));
    rig.browser.select_row(0);
    rig.browser.click_load(0);
    let alpha = rig.row(0);
    let name = Path::new(&alpha.path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        rig.browser.status(),
        format!("Couldn't load {name}: file is missing")
    );
    assert!(rig.browser.status_is_error());

    rig.browser.on_load_requested = Some(Box::new(|_, _| Ok(())));
    rig.browser.click_load(0);
    assert!(rig.browser.status().starts_with("Loaded"));
    assert!(rig.browser.request_load(0, &alpha).is_ok());
}

#[test]
fn watch_folder_scope_is_anchored_and_does_not_match_siblings() {
    let mut rig = BrowserRig::new();
    let lib = rig.services.library();
    let seed = Seed {
        title: "",
        artist: "A",
        bpm: Some(100.0),
        camelot: "1A",
        genre: "",
        rating: 0,
    };
    let track = |title: &'static str, path: &str| {
        add_track(
            lib,
            &Seed {
                title,
                ..seed_copy(&seed)
            },
            Some(path),
        );
    };
    track("In", "/music/house/in.wav");
    track("Deep", "/music/house/sub/deep.wav");
    track("Sibling", "/music/house2/sib.wav");
    track("Elsewhere", "/x/music/house/e.wav");
    track("Pct", "/music/100%_x/p.wav");
    track("PctOther", "/music/100abx/p.wav");
    lib.watch_folders().add("/music/house").unwrap();
    lib.watch_folders().add("/music/100%_x").unwrap();
    rig.browser.refresh(&rig.services);

    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::WatchFolder, 0, "/music/house"));
    assert_eq!(rig.rows(), 2);
    let titles = [rig.title(0), rig.title(1)];
    assert!(titles.contains(&"In".to_owned()) && titles.contains(&"Deep".to_owned()));

    assert!(rig.browser.select_sidebar(
        &rig.services,
        SidebarKind::WatchFolder,
        0,
        "/music/100%_x"
    ));
    assert_eq!(
        rig.rows(),
        1,
        "LIKE wildcards in the folder name are escaped"
    );
    assert_eq!(rig.title(0), "Pct");

    assert!(rig
        .browser
        .select_sidebar(&rig.services, SidebarKind::AllTracks, 0, ""));
    rig.set_search("folder:/music/house/ -title:deep"); // trailing slash accepted
    assert_eq!(rig.rows(), 1);
}

fn seed_copy(s: &Seed) -> Seed {
    Seed {
        title: s.title,
        artist: s.artist,
        bpm: s.bpm,
        camelot: s.camelot,
        genre: s.genre,
        rating: s.rating,
    }
}

#[test]
fn watch_folders_with_the_same_leaf_name_get_distinct_labels() {
    let rig = BrowserRig::new();
    let lib = rig.services.library();
    lib.watch_folders().add("/a/music").unwrap();
    lib.watch_folders().add("/b/music").unwrap();
    lib.watch_folders().add("/c/other").unwrap();
    let nodes = build_sidebar(lib);
    let kids = &nodes[2].children;
    assert_eq!(kids.len(), 3);
    assert_ne!(kids[0].label, kids[1].label);
    assert!(kids[0].label.contains("/a"));
    assert_eq!(kids[2].label, "other");
}

#[test]
fn rows_are_draggable_with_a_track_payload() {
    let mut rig = BrowserRig::new();
    let description = rig
        .browser
        .model
        .drag_source_description(&[2, 3])
        .expect("selected rows describe a drag");
    assert_eq!(parse_track_drag(&description), Some(rig.ids[2]));
    assert_eq!(rig.browser.model.drag_source_description(&[]), None);
    assert_eq!(parse_track_drag("not a track"), None);
    assert_eq!(describe_track_drag(7), "openmixer.track:7");
}

#[test]
fn imports_rekordbox_xml_through_the_services() {
    let mut rig = BrowserRig::new();
    let events = rig.services.subscribe();

    let report = rig
        .browser
        .import_from(&mut rig.services, Path::new(REKORDBOX_FIXTURE))
        .expect("fixture imports");
    assert!(report.tracks_added > 0);
    let changes = events
        .try_iter()
        .filter(|e| *e == LibraryEvent::LibraryChanged)
        .count();
    assert_eq!(changes, 1);
    assert!(rig.browser.status().starts_with("Imported"));
    rig.browser.flush_pending(&mut rig.services);
    assert!(rig.rows() > 4);
    assert!(
        !rig.browser.sidebar_nodes()[1].children.is_empty(),
        "imported playlists show up"
    );

    let missing = rig
        .browser
        .import_from(&mut rig.services, Path::new("/does/not/exist.xml"));
    assert!(missing.is_err());
    assert!(rig.browser.status().contains("failed"));
}

#[test]
fn in_memory_library_has_no_watch_folders_and_pumps_the_prep_queue() {
    let mut services = make_services(memory_options());
    assert!(services.watch_folders().is_none());
    let dir = tempfile::tempdir().unwrap();
    let wav = write_wav(&dir.path().join("t.wav"), 1.0, 220.0);
    let id = add_track(
        services.library(),
        &Seed {
            title: "T",
            artist: "A",
            bpm: None,
            camelot: "",
            genre: "",
            rating: 0,
        },
        Some(&wav.to_string_lossy()),
    );
    let events = services.subscribe();
    services.prep_queue_mut().enqueue(id, false).unwrap();
    let mut seen = Vec::new();
    assert!(pump_until(&mut services, |_| {
        seen.extend(events.try_iter());
        seen.iter()
            .any(|e| matches!(e, LibraryEvent::PrepEntryChanged(p) if p.status == PrepStatus::Done))
    }));
}

#[test]
fn file_library_scans_watch_folders_and_notifies_the_owner() {
    let dir = tempfile::tempdir().unwrap();
    write_wav(&dir.path().join("music/a.wav"), 1.0, 220.0);
    let mut options = Options::for_path(dir.path().join("db/library.db").to_string_lossy());
    options.analyzers = Some(fake_analyzers());
    options.scan_on_start = false;
    let mut services = make_services(options);
    assert!(services.watch_folders().is_some());
    assert!(dir.path().join("db").is_dir(), "parent folder is created");

    let events = services.subscribe();
    services
        .watch_folders()
        .unwrap()
        .add(&dir.path().join("music"))
        .unwrap();
    let mut changes = 0;
    assert!(pump_until(&mut services, |_| {
        changes += events
            .try_iter()
            .filter(|e| *e == LibraryEvent::LibraryChanged)
            .count();
        changes > 0
    }));
    assert_eq!(services.library().tracks().count().unwrap(), 1);
    assert_eq!(build_sidebar(services.library())[2].children.len(), 1);
}

#[test]
fn default_options_honour_the_library_db_environment_variable() {
    std::env::set_var(
        om_ui::library::LIBRARY_DB_ENV,
        "/tmp/om_env_test/library.db",
    );
    assert_eq!(
        om_ui::library::default_options().db_path,
        "/tmp/om_env_test/library.db"
    );
    std::env::remove_var(om_ui::library::LIBRARY_DB_ENV);
    assert!(Path::new(&om_ui::library::default_options().db_path)
        .ends_with(Path::new("OpenMixer").join("library.db")));
}

#[test]
fn unopenable_database_reports_an_error() {
    let options = Options::for_path("/proc/does/not/exist/library.db");
    let error = LibraryServices::create(options).err();
    assert!(error.is_some_and(|e| !e.is_empty()));
}

#[test]
fn scan_thread_errors_are_delivered_on_the_owner_thread() {
    let dir = tempfile::tempdir().unwrap();
    let owner = std::thread::current().id();
    let errors: Rc<RefCell<Vec<(String, bool)>>> = Rc::default();
    let sink = Rc::clone(&errors);
    let mut options = Options::for_path(dir.path().join("library.db").to_string_lossy());
    options.analyzers = Some(fake_analyzers());
    options.scan_on_start = false;
    options.on_error = Some(Box::new(move |m| {
        sink.borrow_mut()
            .push((m.to_owned(), std::thread::current().id() == owner));
    }));
    let mut services = make_services(options);
    services
        .library()
        .database()
        .exec("INSERT INTO schema_version (version, name, applied_at) VALUES (99, 'future', 0);")
        .unwrap();

    services.watch_folders().unwrap().trigger_scan(); // the scan thread cannot open the "newer" database
    assert!(services
        .watch_folders()
        .unwrap()
        .wait_for_scans(1, std::time::Duration::from_secs(8)));
    assert!(pump_until(&mut services, |_| !errors.borrow().is_empty()));
    let first = errors.borrow()[0].clone();
    assert!(first.1, "delivered on the owner thread");
    assert!(first.0.contains("newer"));
    drop(services);
}
