//! Query parser, SQL compilation and results over a seeded
//! library. Fixtures are built with the DAOs in a temporary database file.

use om_library::{
    compile_search, core_migrations, parse_query, CompileContext, FilterTerm, Library,
    LibraryCapabilities, OpenMode, SearchEngine, SearchField, SearchFieldRegistry, SearchQuery,
    SearchValue, TrackRecord, Transaction,
};
use std::sync::Arc;
use tempfile::TempDir;

/// 2024-03-21 00:00 UTC: the clock every result test uses.
const NOW_MS: i64 = 1_710_979_200_000;
const JAN_10: i64 = 1_704_844_800_000;
const FEB_01: i64 = 1_706_745_600_000;
const MAR_01: i64 = 1_709_251_200_000;
const MAR_15: i64 = 1_710_460_800_000;
const MAR_20: i64 = 1_710_892_800_000;

struct Seed {
    title: &'static str,
    artist: &'static str,
    genre: &'static str,
    bpm: Option<f64>,
    camelot: &'static str,
    rating: i32,
    added: i64,
    comment: &'static str,
}

const SEEDS: [Seed; 5] = [
    Seed {
        title: "Alpha",
        artist: "Daft",
        genre: "House",
        bpm: Some(124.0),
        camelot: "8A",
        rating: 5,
        added: JAN_10,
        comment: "",
    },
    Seed {
        title: "Bravo",
        artist: "Moby",
        genre: "Techno",
        bpm: Some(128.0),
        camelot: "9A",
        rating: 3,
        added: FEB_01,
        comment: "",
    },
    Seed {
        title: "Charlie",
        artist: "Moby",
        genre: "House",
        bpm: Some(132.0),
        camelot: "8B",
        rating: 4,
        added: MAR_01,
        comment: "",
    },
    Seed {
        title: "Delta",
        artist: "100%_Pure",
        genre: "Ambient",
        bpm: None,
        camelot: "",
        rating: 0,
        added: MAR_15,
        comment: "late night",
    },
    Seed {
        title: "Echo",
        artist: "Daft",
        genre: "Techno",
        bpm: Some(140.0),
        camelot: "3A",
        rating: 1,
        added: MAR_20,
        comment: "",
    },
];

struct Fixture {
    _dir: TempDir,
    lib: Library,
    peak_id: i64,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("library.db");
        let lib = Library::open(
            path.to_str().expect("utf-8 path"),
            OpenMode::ReadWrite,
            core_migrations(),
        )
        .expect("open library");

        let mut ids = Vec::new();
        for s in &SEEDS {
            let track = TrackRecord {
                path: format!("/m/{}.wav", s.title),
                title: s.title.into(),
                artist: s.artist.into(),
                genre: s.genre.into(),
                bpm: s.bpm,
                camelot: s.camelot.into(),
                rating: s.rating,
                date_added_ms: s.added,
                comment: s.comment.into(),
                ..Default::default()
            };
            ids.push(lib.tracks().insert(&track).expect("insert track"));
        }
        let playlists = lib.playlists();
        let warm = playlists.create("Warmup", None, false).expect("playlist");
        let peak = playlists.create("Peak", None, false).expect("playlist");
        playlists.add_track(warm, ids[0], -1).expect("add");
        playlists.add_track(warm, ids[1], -1).expect("add");
        playlists.add_track(peak, ids[2], -1).expect("add");

        Self {
            _dir: dir,
            lib,
            peak_id: peak,
        }
    }

    fn engine(&self) -> SearchEngine<&om_library::Database> {
        let mut engine = SearchEngine::new(self.lib.database());
        engine.clock = Arc::new(|| NOW_MS);
        engine
    }

    fn titles(&self, q: &str) -> Vec<String> {
        self.engine()
            .search_text(q)
            .unwrap_or_else(|e| panic!("query {q:?} failed: {e}"))
            .into_iter()
            .map(|r| r.title)
            .collect()
    }

    fn joined(&self, q: &str) -> String {
        self.titles(q).join(",")
    }
}

// ---- parser ----

#[test]
fn parser_splits_fields_free_text_and_quotes() {
    let fields = SearchFieldRegistry::with_defaults();
    let p = parse_query("artist:foo \"deep house\" bpm:120-128 12:30", &fields);
    assert_eq!(p.query.terms.len(), 4);
    assert_eq!(p.query.terms[0].field, "artist");
    assert_eq!(p.query.terms[0].value, "foo");
    assert!(p.query.terms[1].field.is_empty());
    assert_eq!(p.query.terms[1].value, "deep house");
    assert_eq!(p.query.terms[2].value, "120-128");
    assert!(p.query.terms[3].field.is_empty()); // unknown prefix stays free text
    assert_eq!(p.query.terms[3].value, "12:30");
    assert!(p.errors.is_empty());
}

#[test]
fn parser_quoted_field_value_and_negation() {
    let fields = SearchFieldRegistry::with_defaults();
    let p = parse_query("-genre:techno title:\"a b\" -\"x y\"", &fields);
    assert_eq!(p.query.terms.len(), 3);
    assert!(p.query.terms[0].negated);
    assert_eq!(p.query.terms[0].field, "genre");
    assert!(!p.query.terms[1].negated);
    assert_eq!(p.query.terms[1].value, "a b");
    assert!(p.query.terms[2].negated);
    assert_eq!(p.query.terms[2].value, "x y");
}

#[test]
fn parser_sort_limit_offset_are_reserved() {
    let fields = SearchFieldRegistry::with_defaults();
    let p = parse_query("sort:-BPM limit:10 offset:20 house", &fields);
    assert_eq!(p.query.sort_field, "bpm");
    assert!(p.query.descending);
    assert_eq!(p.query.limit, Some(10));
    assert_eq!(p.query.offset, 20);
    assert_eq!(p.query.terms.len(), 1);
}

#[test]
fn parser_bad_limit_is_reported_not_ignored() {
    let fields = SearchFieldRegistry::with_defaults();
    let p = parse_query("limit:abc offset:-3 house", &fields);
    assert_eq!(p.errors.len(), 2);
    assert_eq!(p.query.terms.len(), 1);
}

#[test]
fn parser_empty_and_whitespace_queries_have_no_terms() {
    let fields = SearchFieldRegistry::with_defaults();
    assert!(parse_query("", &fields).query.terms.is_empty());
    assert!(parse_query("   \t ", &fields).query.terms.is_empty());
}

// ---- results ----

#[test]
fn empty_query_returns_everything_by_id() {
    let f = Fixture::new();
    assert_eq!(f.joined(""), "Alpha,Bravo,Charlie,Delta,Echo");
}

#[test]
fn free_text_searches_all_text_fields_case_insensitively() {
    let f = Fixture::new();
    assert_eq!(f.joined("MOBY"), "Bravo,Charlie");
    assert_eq!(f.joined("late"), "Delta"); // comment
    assert_eq!(f.joined("ambient"), "Delta"); // genre
    assert_eq!(f.joined("moby house"), "Charlie"); // terms AND together
}

#[test]
fn field_filters() {
    let f = Fixture::new();
    assert_eq!(f.joined("artist:daft"), "Alpha,Echo");
    assert_eq!(f.joined("genre:=house"), "Alpha,Charlie");
    assert_eq!(f.joined("-genre:techno artist:moby"), "Charlie");
    assert_eq!(f.joined("title:\"lph\""), "Alpha");
}

#[test]
fn like_wildcards_in_user_text_are_literal() {
    let f = Fixture::new();
    assert_eq!(f.joined("artist:%"), "Delta");
    assert_eq!(f.joined("artist:_"), "Delta");
    assert_eq!(f.joined("artist:0%_P"), "Delta");
}

#[test]
fn bpm_ranges_and_comparisons() {
    let f = Fixture::new();
    assert_eq!(f.joined("bpm:124-128"), "Alpha,Bravo");
    assert_eq!(f.joined("bpm:128..140"), "Bravo,Charlie,Echo");
    assert_eq!(f.joined("bpm:>128"), "Charlie,Echo");
    assert_eq!(f.joined("bpm:<=128"), "Alpha,Bravo");
    assert_eq!(f.joined("bpm:132"), "Charlie");
    assert_eq!(f.joined("-bpm:124-132"), "Delta,Echo"); // unanalysed tracks are outside any range
}

#[test]
fn rating_filter() {
    let f = Fixture::new();
    assert_eq!(f.joined("rating:>=4"), "Alpha,Charlie");
    assert_eq!(f.joined("rating:0"), "Delta");
}

#[test]
fn key_exact_and_compatible() {
    let f = Fixture::new();
    assert_eq!(f.joined("key:8A"), "Alpha");
    assert_eq!(f.joined("key:8a"), "Alpha");
    assert_eq!(f.joined("key:Am"), "Alpha"); // musical name resolves to 8A
    assert_eq!(f.joined("key:~8A"), "Alpha,Bravo,Charlie"); // 8A, 9A (+1), 8B (relative)
    assert_eq!(f.joined("-key:~8A"), "Delta,Echo");
}

#[test]
fn date_added_absolute_and_relative() {
    let f = Fixture::new();
    assert_eq!(f.joined("added:2024-03-01"), "Charlie");
    assert_eq!(f.joined("added:>2024-03-01"), "Delta,Echo");
    assert_eq!(f.joined("added:>=2024-03-01"), "Charlie,Delta,Echo");
    assert_eq!(f.joined("added:2024-02-01..2024-03-01"), "Bravo,Charlie");
    assert_eq!(f.joined("added:7d"), "Delta,Echo"); // clock is 2024-03-21
    assert_eq!(f.joined("added:<2024-02-01"), "Alpha");
}

#[test]
fn playlist_membership_by_name_and_id() {
    let f = Fixture::new();
    assert_eq!(f.joined("playlist:warmup"), "Alpha,Bravo");
    assert_eq!(f.joined("playlist:=Peak"), "Charlie");
    assert_eq!(f.joined(&format!("playlist:#{}", f.peak_id)), "Charlie");
    assert_eq!(f.joined("-playlist:warmup"), "Charlie,Delta,Echo");
}

#[test]
fn sorts_by_any_registered_column() {
    let f = Fixture::new();
    assert_eq!(f.joined("sort:-bpm"), "Echo,Charlie,Bravo,Alpha,Delta");
    assert_eq!(f.joined("sort:rating"), "Delta,Echo,Bravo,Charlie,Alpha");
    assert_eq!(f.joined("sort:-title"), "Echo,Delta,Charlie,Bravo,Alpha");
    // 3A 8A 8B 9A: wheel number, then letter.
    assert_eq!(f.joined("sort:key bpm:>0"), "Echo,Alpha,Charlie,Bravo");
    assert_eq!(f.joined("sort:artist"), "Delta,Alpha,Echo,Bravo,Charlie");
}

#[test]
fn limit_offset_page_and_count_ignores_paging() {
    let f = Fixture::new();
    assert_eq!(f.joined("limit:2"), "Alpha,Bravo");
    assert_eq!(f.joined("limit:2 offset:2"), "Charlie,Delta");
    assert_eq!(f.joined("offset:4"), "Echo");
    assert_eq!(f.joined("offset:99"), "");

    let engine = f.engine();
    let q = engine.parse("genre:house limit:1").query;
    assert_eq!(engine.count(&q).expect("count"), 2);
}

// ---- failure reporting ----

#[test]
fn bad_queries_fail_instead_of_returning_no_results() {
    let f = Fixture::new();
    let engine = f.engine();
    for q in [
        "sort:nonsense",
        "bpm:fast",
        "key:H#",
        "added:yesterday",
        "playlist:#x",
        "limit:zzz",
        "sort:",
        "sort:-",
    ] {
        assert!(engine.search_text(q).is_err(), "{q} should fail");
    }
}

#[test]
fn unknown_field_in_hand_built_query_fails() {
    let f = Fixture::new();
    let engine = f.engine();
    let q = SearchQuery {
        terms: vec![FilterTerm {
            field: "nope".into(),
            value: "x".into(),
            negated: false,
        }],
        ..Default::default()
    };
    assert!(engine.search(&q).is_err());
    assert!(engine.count(&q).is_err());
}

#[test]
fn database_failure_is_surfaced_not_empty() {
    let f = Fixture::new();
    f.lib
        .database()
        .exec("ALTER TABLE tracks RENAME TO tracks_gone;")
        .expect("rename");
    let engine = f.engine();
    assert!(engine.search_text("alpha").is_err());
    assert!(engine.count(&SearchQuery::default()).is_err());
}

// ---- injection safety ----

#[test]
fn hostile_values_are_data_not_sql() {
    let f = Fixture::new();
    let engine = f.engine();
    let hostile = [
        "'; DROP TABLE tracks; --",
        "\" OR 1=1 --",
        "x') OR ('1'='1",
        "Robert'); DELETE FROM tracks;--",
    ];
    for h in hostile {
        for field in [
            "",
            "title:",
            "artist:",
            "genre:=",
            "playlist:",
            "key:",
            "bpm:",
            "added:",
        ] {
            // Either matches nothing, or fails to parse a number, key or date.
            if let Ok(rows) = engine.search_text(&format!("{field}\"{h}\"")) {
                assert!(rows.is_empty(), "{field}{h} matched rows");
            }
        }
    }
    assert_eq!(f.lib.tracks().count().expect("count"), 5);
    assert_eq!(f.joined(""), "Alpha,Bravo,Charlie,Delta,Echo");
}

#[test]
fn hostile_sort_field_is_rejected() {
    let f = Fixture::new();
    let engine = f.engine();
    let q = SearchQuery {
        sort_field: "id; DROP TABLE tracks".into(),
        ..Default::default()
    };
    assert!(engine.search(&q).is_err());
    assert_eq!(f.lib.tracks().count().expect("count"), 5);
}

#[test]
fn compiled_sql_never_contains_user_text() {
    let fields = SearchFieldRegistry::with_defaults();
    let p = parse_query(
        "artist:\"zz9plural'; --\" bpm:120-128 key:~8A playlist:\"zz9z\" sort:-bpm",
        &fields,
    );
    let c = compile_search(&p.query, &fields, &CompileContext::default()).expect("compile");
    assert!(!c.select_sql.contains("zz9"));
    assert!(!c.select_sql.contains("120"));
    assert!(!c.select_sql.contains("8A"));
    let placeholders = c.select_sql.matches('?').count();
    assert_eq!(placeholders, c.select_args.len());
}

// ---- extension point ----

#[test]
fn plugin_can_register_a_field() {
    let f = Fixture::new();
    let mut engine = f.engine();
    engine
        .registry_mut()
        .add(SearchField::custom("has-comment", &[], |term, out| {
            out.sql.push_str(if term.value == "no" {
                "tracks.comment = ''"
            } else {
                "tracks.comment <> ''"
            });
            Ok(())
        }));

    let rows = |q: &str| -> String {
        engine
            .search_text(q)
            .expect("search")
            .into_iter()
            .map(|r| r.title)
            .collect::<Vec<_>>()
            .join(",")
    };
    assert_eq!(rows("has-comment:yes"), "Delta");
    assert_eq!(rows("has-comment:no"), "Alpha,Bravo,Charlie,Echo");
}

#[test]
fn migration_adds_indexes() {
    let f = Fixture::new();
    let db = f.lib.database();
    let mut s = db
        .prepare("SELECT COUNT(*) FROM sqlite_master WHERE name='idx_tracks_bpm';")
        .expect("prepare");
    let n: i64 = s.query_row([], |row| row.get(0)).expect("count");
    assert_eq!(n, 1);
}

#[test]
fn folder_term_is_anchored_and_escaped() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("library.db");
    let lib = Library::open(
        path.to_str().expect("utf-8 path"),
        OpenMode::ReadWrite,
        core_migrations(),
    )
    .expect("open library");
    for path in [
        "/m/house/a.wav",
        "/m/house/sub/b.wav",
        "/m/house2/c.wav",
        "/x/m/house/d.wav",
        "/m/h_use/e.wav",
        "/m/hxuse/f.wav",
    ] {
        let t = TrackRecord {
            path: path.into(),
            title: path.into(),
            ..Default::default()
        };
        lib.tracks().insert(&t).expect("insert");
    }
    let engine = SearchEngine::new(lib.database());
    let count = |q: &str| engine.search_text(q).expect(q).len();
    assert_eq!(count("folder:/m/house"), 2);
    assert_eq!(count("folder:/m/house/"), 2);
    assert_eq!(count("folder:/m/h_use"), 1); // '_' is literal
    assert_eq!(count("-folder:/m/house"), 4);
    assert_eq!(count("folder:/m"), 5);
}

#[test]
fn compiled_count_and_page_args_are_bound_in_order() {
    let fields = SearchFieldRegistry::with_defaults();
    let q = SearchQuery {
        limit: Some(3),
        offset: 6,
        ..Default::default()
    };
    let c = compile_search(&q, &fields, &CompileContext::default()).expect("compile");
    assert!(c.count_args.is_empty());
    assert_eq!(
        c.select_args,
        vec![SearchValue::Int(3), SearchValue::Int(6)]
    );
}

// ---- free text over the FTS5 index ----

/// A library in a temporary file, with the given `(title, artist, genre)` rows.
fn library_with(rows: &[(&str, &str, &str)], migrations: &[om_library::Migration]) -> (TempDir, Library) {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("library.db");
    let lib = Library::open(path.to_str().expect("utf-8 path"), OpenMode::ReadWrite, migrations)
        .expect("open library");
    for (i, (title, artist, genre)) in rows.iter().enumerate() {
        let track = TrackRecord {
            path: format!("/f/{i}.wav"),
            title: (*title).into(),
            artist: (*artist).into(),
            genre: (*genre).into(),
            ..Default::default()
        };
        lib.tracks().insert(&track).expect("insert");
    }
    (dir, lib)
}

fn titles_of(engine: &SearchEngine<&om_library::Database>, q: &str) -> Vec<String> {
    let mut titles: Vec<String> = engine
        .search_text(q)
        .unwrap_or_else(|e| panic!("query {q:?} failed: {e}"))
        .into_iter()
        .map(|r| r.title)
        .collect();
    titles.sort();
    titles
}

#[test]
fn free_text_folds_diacritics_and_matches_token_prefixes() {
    let (_dir, lib) = library_with(
        &[("Émile Hymne", "Daft", "House"), ("Love Song", "Moby", "Techno")],
        core_migrations(),
    );
    let engine = SearchEngine::new(lib.database());
    assert!(engine.capabilities.fts5, "a fresh library has the FTS5 index");
    assert_eq!(titles_of(&engine, "emile"), ["Émile Hymne"]);
    assert_eq!(titles_of(&engine, "ÉMILE"), ["Émile Hymne"]);
    assert_eq!(titles_of(&engine, "hym"), ["Émile Hymne"], "a prefix of a token");
    assert!(titles_of(&engine, "ymne").is_empty(), "a token suffix is not a match");
    assert_eq!(titles_of(&engine, "love mo"), ["Love Song"], "each term may match any column");
}

#[test]
fn quoted_phrase_matches_adjacent_tokens_only() {
    let (_dir, lib) = library_with(
        &[
            ("A", "x", "Deep House"),
            ("B", "x", "House Deep"),
            ("C", "x", "Deep Techno House"),
        ],
        core_migrations(),
    );
    let engine = SearchEngine::new(lib.database());
    assert_eq!(titles_of(&engine, "\"deep house\""), ["A"]);
    assert_eq!(titles_of(&engine, "deep"), ["A", "B", "C"]);
    assert_eq!(titles_of(&engine, "house deep"), ["A", "B", "C"], "separate terms AND together");
    assert_eq!(titles_of(&engine, "-deep"), Vec::<String>::new());
}

#[test]
fn hostile_free_text_is_plain_text_and_never_errors() {
    let (_dir, lib) = library_with(
        &[("House Mix", "NEAR Band", "Ambient"), ("Plain", "Quiet", "Ambient")],
        core_migrations(),
    );
    let engine = SearchEngine::new(lib.database());
    let hostile = [
        "\"", "\"\"", "\"unbalanced", "*", "**", "^", "NEAR", "NEAR(", "NEAR/2", "OR", "AND",
        "NOT", "a OR b", ")(", "()", "x)(y", "%", "_", "\\", "'; DROP TABLE tracks; --",
        "\"x\" * NEAR OR \"", "title:\"", "-", "-\"", "{a}", "+x", ":",
    ];
    for h in hostile {
        engine
            .search_text(h)
            .unwrap_or_else(|e| panic!("{h:?} errored: {e}"));
        engine
            .count(&engine.parse(h).query)
            .unwrap_or_else(|e| panic!("count of {h:?} errored: {e}"));
    }
    // Operators are words here: "OR" is a token that no track contains.
    assert!(titles_of(&engine, "house OR techno").is_empty());
    assert_eq!(titles_of(&engine, "house"), ["House Mix"]);
    assert_eq!(titles_of(&engine, "NEAR"), ["House Mix"]); // the artist "NEAR Band"
    assert_eq!(lib.tracks().count().expect("count"), 2);
}

#[test]
fn index_follows_insert_update_and_delete_and_rescan() {
    let (_dir, lib) = library_with(&[("Alpha", "Daft", "House")], core_migrations());
    let tracks = lib.tracks();
    let id = tracks.find_by_path("/f/0.wav").expect("find").expect("row").id;
    {
        let engine = SearchEngine::new(lib.database());
        assert_eq!(titles_of(&engine, "alpha"), ["Alpha"]);
    }

    // Update of an indexed text column: the old token goes, the new one comes.
    let mut t = tracks.find_by_id(id).expect("find").expect("row");
    t.title = "Bravo".into();
    tracks.update(&t).expect("update");
    {
        let engine = SearchEngine::new(lib.database());
        assert!(titles_of(&engine, "alpha").is_empty());
        assert_eq!(titles_of(&engine, "bravo"), ["Bravo"]);
    }

    // A rescan refreshes by path (an UPDATE on the same row).
    tracks
        .upsert_by_path(&TrackRecord {
            path: "/f/0.wav".into(),
            title: "Charlie".into(),
            artist: "Moby".into(),
            ..Default::default()
        })
        .expect("upsert");
    {
        let engine = SearchEngine::new(lib.database());
        assert!(titles_of(&engine, "bravo").is_empty());
        assert_eq!(titles_of(&engine, "moby"), ["Charlie"]);
    }

    tracks.remove(id).expect("remove");
    let engine = SearchEngine::new(lib.database());
    assert!(titles_of(&engine, "charlie").is_empty());
    assert!(titles_of(&engine, "moby").is_empty());
    let fts_rows: i64 = lib
        .database()
        .connection()
        .query_row("SELECT COUNT(*) FROM tracks_fts_docsize;", [], |r| r.get(0))
        .expect("docsize");
    assert_eq!(fts_rows, 0, "no index entries remain after the delete");
}

#[test]
fn free_text_falls_back_to_like_when_the_fts_index_is_missing_at_run_time() {
    let (_dir, lib) = library_with(
        &[("Alpha", "Moby", "House"), ("Bravo", "Daft", "Techno")],
        core_migrations(),
    );
    let engine = SearchEngine::new(lib.database());
    assert!(engine.capabilities.fts5);
    // The index disappears after the engine was created: the search must still answer.
    lib.database()
        .exec("DROP TABLE tracks_fts;")
        .expect("drop index");
    assert_eq!(titles_of(&engine, "moby"), ["Alpha"]);
    assert_eq!(titles_of(&engine, "echno"), ["Bravo"], "LIKE substring again");
}

#[test]
fn free_text_uses_like_when_fts5_is_disabled_and_both_paths_agree() {
    let (_dir, lib) = library_with(
        &[("Alpha", "Moby", "House"), ("Bravo", "Daft", "Techno"), ("Charlie", "Moby", "House")],
        core_migrations(),
    );
    let fts = SearchEngine::new(lib.database());
    let mut like = SearchEngine::new(lib.database());
    like.capabilities = LibraryCapabilities { fts5: false };
    for q in ["moby", "house", "techno", "-house", "moby -house", "alpha"] {
        assert_eq!(titles_of(&fts, q), titles_of(&like, q), "query {q:?}");
    }
    // The LIKE form matches substrings inside a word, which the token index does not.
    assert_eq!(titles_of(&like, "echno"), ["Bravo"]);
    assert!(titles_of(&fts, "echno").is_empty());
}

#[test]
fn library_without_the_index_detects_no_fts5_and_searches_with_like() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("v6.db");
    let path = path.to_str().expect("utf-8 path");
    {
        let lib = Library::open(path, OpenMode::ReadWrite, &core_migrations()[..6]).expect("v6");
        lib.tracks()
            .insert(&TrackRecord {
                path: "/v6/a.wav".into(),
                title: "Moby Dick".into(),
                ..Default::default()
            })
            .expect("insert");
    }
    let lib = Library::open(path, OpenMode::ReadWrite, core_migrations()).expect("upgrade");
    let v6 = Library::open(path, OpenMode::ReadOnly, core_migrations()).expect("read-only");
    assert!(LibraryCapabilities::detect(v6.database()).fts5, "read-only sees the migrated index");
    let engine = SearchEngine::new(lib.database());
    assert!(titles_of(&engine, "oby").is_empty(), "token prefix, not substring");
    assert_eq!(titles_of(&engine, "moby"), ["Moby Dick"]);

    let (_d2, old) = {
        let d = TempDir::new().expect("temp dir");
        let p = d.path().join("old.db");
        let l = Library::open(p.to_str().expect("path"), OpenMode::ReadWrite, &core_migrations()[..6])
            .expect("v6");
        l.tracks()
            .insert(&TrackRecord {
                path: "/old.wav".into(),
                title: "Moby Dick".into(),
                ..Default::default()
            })
            .expect("insert");
        (d, l)
    };
    assert!(!LibraryCapabilities::detect(old.database()).fts5, "no tracks_fts table");
    let old_engine = SearchEngine::new(old.database());
    assert_eq!(titles_of(&old_engine, "oby"), ["Moby Dick"], "LIKE substring");
}

#[test]
fn free_text_on_twenty_thousand_tracks_answers_quickly() {
    let (_dir, lib) = library_with(&[], core_migrations());
    {
        let tx = Transaction::begin(lib.database()).expect("begin");
        for i in 0..20_000u32 {
            let title = if i % 5_000 == 17 {
                format!("Needle {i}")
            } else {
                format!("Track {i} Mix")
            };
            lib.tracks()
                .insert(&TrackRecord {
                    path: format!("/bulk/{i}.wav"),
                    title,
                    artist: format!("Artist {}", i % 300),
                    genre: if i % 2 == 0 { "House" } else { "Techno" }.into(),
                    ..Default::default()
                })
                .expect("insert");
        }
        tx.commit().expect("commit");
    }
    let engine = SearchEngine::new(lib.database());
    let start = std::time::Instant::now();
    let rows = engine.search_text("needle").expect("search");
    let elapsed = start.elapsed();
    assert_eq!(rows.len(), 4);
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "free-text search took {elapsed:?} over 20k tracks"
    );
    // Artist 7, 307, ... : 67 of the 20 000 rows.
    assert_eq!(engine.count(&engine.parse("artist:\"=Artist 7\"").query).expect("count"), 67);
}
