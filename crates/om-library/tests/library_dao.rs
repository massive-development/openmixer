//! Track, cue/loop and playlist DAOs.

use om_library::{core_migrations, CuePoint, Library, OpenMode, Playlist, SavedLoop, TrackRecord};

fn memory_library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open :memory:")
}

fn sample_track(path: &str) -> TrackRecord {
    TrackRecord {
        path: path.into(),
        content_hash: format!("hash-{path}"),
        title: "Title".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        genre: "House".into(),
        year: 2020,
        duration_sec: 215.5,
        bpm: Some(124.0),
        key: "Am".into(),
        camelot: "8A".into(),
        rating: 4,
        play_count: 3,
        date_added_ms: 1000,
        last_played_ms: Some(2000),
        comment: "note".into(),
        ..Default::default()
    }
}

fn add_track(lib: &Library, path: &str) -> i64 {
    lib.tracks()
        .insert(&sample_track(path))
        .expect("insert track")
}

fn track_order(lib: &Library, playlist_id: i64) -> Vec<i64> {
    lib.playlists()
        .entries(playlist_id)
        .expect("entries")
        .iter()
        .map(|e| e.track_id)
        .collect()
}

fn names(list: &[Playlist]) -> Vec<String> {
    list.iter().map(|p| p.name.clone()).collect()
}

mod library_tracks {
    use super::*;

    #[test]
    fn insert_and_find_round_trips_every_field() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");

        let got = lib.tracks().find_by_id(id).unwrap().expect("found");
        let want = sample_track("/m/a.wav");
        assert_eq!(got.path, want.path);
        assert_eq!(got.content_hash, want.content_hash);
        assert_eq!(got.title, want.title);
        assert_eq!(got.artist, want.artist);
        assert_eq!(got.album, want.album);
        assert_eq!(got.genre, want.genre);
        assert_eq!(got.year, 2020);
        assert_eq!(got.duration_sec, 215.5);
        assert_eq!(got.bpm.unwrap_or(0.0), 124.0);
        assert_eq!(got.key, "Am");
        assert_eq!(got.camelot, "8A");
        assert_eq!(got.rating, 4);
        assert_eq!(got.play_count, 3);
        assert_eq!(got.date_added_ms, 1000);
        assert_eq!(got.last_played_ms.unwrap_or(0), 2000);
        assert_eq!(got.comment, "note");
    }

    #[test]
    fn unicode_and_nullable_fields_survive() {
        let lib = memory_library();
        let track = TrackRecord {
            path: "/m/\u{266B}.wav".into(),
            title: "Caf\u{e9} \u{266B}".into(),
            ..Default::default()
        };
        lib.tracks().insert(&track).expect("insert");

        let got = lib
            .tracks()
            .find_by_path(&track.path)
            .unwrap()
            .expect("found");
        assert_eq!(got.title, track.title);
        assert!(got.bpm.is_none());
        assert!(got.last_played_ms.is_none());
        assert!(got.date_added_ms > 0, "defaulted to now");
    }

    #[test]
    fn duplicate_path_is_rejected() {
        let lib = memory_library();
        add_track(&lib, "/m/a.wav");
        assert!(lib.tracks().insert(&sample_track("/m/a.wav")).is_err());
        assert_eq!(lib.tracks().count().unwrap(), 1);
    }

    #[test]
    fn update_and_narrow_writers() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");
        let mut t = lib.tracks().find_by_id(id).unwrap().unwrap();
        t.title = "New".into();
        t.bpm = None;
        lib.tracks().update(&t).expect("update");
        assert_eq!(lib.tracks().find_by_id(id).unwrap().unwrap().title, "New");
        assert!(lib.tracks().find_by_id(id).unwrap().unwrap().bpm.is_none());

        lib.tracks()
            .set_analysis(id, 128.0, "C", "8B")
            .expect("set_analysis");
        lib.tracks().set_rating(id, 5).expect("set_rating");
        lib.tracks()
            .record_play(id, None, 9999)
            .expect("record_play");
        let got = lib.tracks().find_by_id(id).unwrap().unwrap();
        assert_eq!(got.bpm.unwrap(), 128.0);
        assert_eq!(got.camelot, "8B");
        assert_eq!(got.rating, 5);
        assert_eq!(got.play_count, 4);
        assert_eq!(got.last_played_ms.unwrap(), 9999);
    }

    #[test]
    fn rating_out_of_range_and_unknown_id_fail() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");
        assert!(lib.tracks().set_rating(id, 6).is_err());
        assert!(lib.tracks().set_rating(id + 100, 3).is_err());
        assert!(lib.tracks().remove(id + 100).is_err());
    }

    #[test]
    fn upsert_keeps_user_fields_and_id() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");

        let mut refreshed = sample_track("/m/a.wav");
        refreshed.title = "Retagged".into();
        refreshed.rating = 0;
        refreshed.play_count = 0;
        let again = lib.tracks().upsert_by_path(&refreshed).expect("upsert");
        assert_eq!(again, id);

        let got = lib.tracks().find_by_id(id).unwrap().unwrap();
        assert_eq!(got.title, "Retagged");
        assert_eq!(got.rating, 4);
        assert_eq!(got.play_count, 3);
        assert_eq!(lib.tracks().count().unwrap(), 1);

        assert!(lib
            .tracks()
            .upsert_by_path(&sample_track("/m/b.wav"))
            .is_ok());
        assert_eq!(lib.tracks().count().unwrap(), 2);
    }

    #[test]
    fn find_by_content_hash_returns_all_copies() {
        let lib = memory_library();
        let mut a = sample_track("/m/a.wav");
        let mut b = sample_track("/m/b.wav");
        a.content_hash = "same".into();
        b.content_hash = "same".into();
        lib.tracks().insert(&a).unwrap();
        lib.tracks().insert(&b).unwrap();
        assert_eq!(lib.tracks().find_by_content_hash("same").unwrap().len(), 2);
        assert!(lib
            .tracks()
            .find_by_content_hash("other")
            .unwrap()
            .is_empty());
        assert_eq!(lib.tracks().all().unwrap().len(), 2);
    }

    #[test]
    fn empty_string_hash_lookup_matches_default_column() {
        let lib = memory_library();
        let track = TrackRecord {
            path: "/m/nohash.wav".into(), // content_hash defaults to ''
            ..Default::default()
        };
        lib.tracks().insert(&track).expect("insert");

        assert_eq!(lib.tracks().find_by_content_hash("").unwrap().len(), 1);
        assert!(lib.tracks().find_by_path("").unwrap().is_none());
    }

    #[test]
    fn find_by_id_uses_bound_parameter() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");
        assert!(lib.tracks().find_by_id(id).unwrap().is_some());
        assert!(lib.tracks().find_by_id(id + 1).unwrap().is_none());
    }
}

mod library_cues {
    use super::*;

    #[test]
    fn set_replace_list_and_clear() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");
        let cues = lib.cues();
        cues.set_cue(&CuePoint {
            track_id: id,
            slot: 2,
            position_frames: 44100,
            colour_argb: 0xFFFF_0000,
            label: "drop".into(),
        })
        .unwrap();
        cues.set_cue(&CuePoint {
            track_id: id,
            slot: 0,
            position_frames: 100,
            colour_argb: 0xFF00_FF00,
            label: "intro".into(),
        })
        .unwrap();
        cues.set_cue(&CuePoint {
            track_id: id,
            slot: 2,
            position_frames: 88200,
            colour_argb: 0xFF00_00FF,
            label: "drop2".into(),
        })
        .unwrap();

        let list = cues.cues(id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].slot, 0);
        assert_eq!(list[1].position_frames, 88200);
        assert_eq!(list[1].colour_argb, 0xFF00_00FF);
        assert_eq!(list[1].label, "drop2");

        cues.clear_cue(id, 0).unwrap();
        assert_eq!(cues.cues(id).unwrap().len(), 1);
    }

    #[test]
    fn loops_round_trip_and_reject_inverted_range() {
        let lib = memory_library();
        let id = add_track(&lib, "/m/a.wav");
        let cues = lib.cues();
        cues.set_loop(&SavedLoop {
            track_id: id,
            slot: 1,
            start_frames: 1000,
            end_frames: 5000,
            colour_argb: 0xFFAA_BBCC,
            label: "loop".into(),
        })
        .unwrap();
        assert!(cues
            .set_loop(&SavedLoop {
                track_id: id,
                slot: 2,
                start_frames: 5000,
                end_frames: 1000,
                ..Default::default()
            })
            .is_err());

        let list = cues.loops(id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].start_frames, 1000);
        assert_eq!(list[0].end_frames, 5000);
        cues.clear_loop(id, 1).unwrap();
        assert!(cues.loops(id).unwrap().is_empty());
    }

    #[test]
    fn unknown_track_fails_and_delete_cascades() {
        let lib = memory_library();
        let orphan = CuePoint {
            track_id: 999,
            slot: 0,
            position_frames: 1,
            ..Default::default()
        };
        assert!(lib.cues().set_cue(&orphan).is_err());

        let id = add_track(&lib, "/m/a.wav");
        lib.cues()
            .set_cue(&CuePoint {
                track_id: id,
                slot: 0,
                position_frames: 1,
                ..Default::default()
            })
            .unwrap();
        lib.cues()
            .set_loop(&SavedLoop {
                track_id: id,
                slot: 0,
                start_frames: 1,
                end_frames: 2,
                ..Default::default()
            })
            .unwrap();
        lib.tracks().remove(id).expect("remove");
        assert!(lib.cues().cues(id).unwrap().is_empty());
        assert!(lib.cues().loops(id).unwrap().is_empty());
    }
}

mod library_playlists {
    use super::*;

    #[test]
    fn entries_append_insert_move_remove_keep_dense_positions() {
        let lib = memory_library();
        let pl = lib.playlists().create("Set", None, false).expect("create");
        let a = add_track(&lib, "/a");
        let b = add_track(&lib, "/b");
        let c = add_track(&lib, "/c");
        let d = add_track(&lib, "/d");

        let p = lib.playlists();
        let ea = p.add_track(pl, a, -1).unwrap();
        p.add_track(pl, b, -1).unwrap();
        p.add_track(pl, c, -1).unwrap();
        assert_eq!(track_order(&lib, pl), vec![a, b, c]);

        p.add_track(pl, d, 1).unwrap();
        assert_eq!(track_order(&lib, pl), vec![a, d, b, c]);

        p.move_entry(ea, 3).unwrap();
        assert_eq!(track_order(&lib, pl), vec![d, b, c, a]);
        p.move_entry(ea, 0).unwrap();
        assert_eq!(track_order(&lib, pl), vec![a, d, b, c]);
        p.move_entry(ea, 99).unwrap(); // clamps to the end
        assert_eq!(track_order(&lib, pl), vec![d, b, c, a]);

        let second = p.entries(pl).unwrap()[1].id;
        p.remove_entry(second).unwrap();
        assert_eq!(track_order(&lib, pl), vec![d, c, a]);
        let remaining = p.entries(pl).unwrap();
        for (i, e) in remaining.iter().enumerate() {
            assert_eq!(e.position, i as i32);
        }
    }

    #[test]
    fn same_track_may_appear_twice_and_deleting_track_removes_entries() {
        let lib = memory_library();
        let pl = lib.playlists().create("Set", None, false).unwrap();
        let a = add_track(&lib, "/a");
        lib.playlists().add_track(pl, a, -1).unwrap();
        lib.playlists().add_track(pl, a, -1).unwrap();
        assert_eq!(lib.playlists().entries(pl).unwrap().len(), 2);

        lib.tracks().remove(a).unwrap();
        assert!(lib.playlists().entries(pl).unwrap().is_empty());
    }

    #[test]
    fn add_track_rejects_folder_and_unknown_track_without_side_effects() {
        let lib = memory_library();
        let folder = lib.playlists().create("F", None, true).unwrap();
        let pl = lib.playlists().create("P", None, false).unwrap();
        let a = add_track(&lib, "/a");
        lib.playlists().add_track(pl, a, -1).unwrap();

        assert!(lib.playlists().add_track(folder, a, -1).is_err());
        assert!(lib.playlists().add_track(pl, 9999, 0).is_err());
        let entries = lib.playlists().entries(pl).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].position, 0, "failed insert at 0 did not shift");
    }

    #[test]
    fn nesting_ordering_and_cascade_delete() {
        let lib = memory_library();
        let p = lib.playlists();
        let folder = p.create("Folder", None, true).unwrap();
        let x = p.create("X", Some(folder), false).unwrap();
        let y = p.create("Y", Some(folder), false).unwrap();
        let z = p.create("Z", Some(folder), false).unwrap();
        assert_eq!(names(&p.children(Some(folder)).unwrap()), ["X", "Y", "Z"]);

        p.move_node(z, Some(folder), 0).unwrap();
        assert_eq!(names(&p.children(Some(folder)).unwrap()), ["Z", "X", "Y"]);

        p.move_node(x, None, 0).unwrap(); // out of the folder, to the top
        assert_eq!(names(&p.children(None).unwrap()), ["X", "Folder"]);
        let inside = p.children(Some(folder)).unwrap();
        assert_eq!(names(&inside), ["Z", "Y"]);
        assert_eq!(inside[1].position, 1);

        let t = add_track(&lib, "/a");
        p.add_track(y, t, -1).unwrap();
        p.remove(folder).unwrap();
        assert!(p.find(y).unwrap().is_none());
        assert!(p.find_entry(1).unwrap().is_none());
        let top = p.children(None).unwrap();
        assert_eq!(names(&top), ["X"]);
        assert_eq!(top[0].position, 0);
    }

    #[test]
    fn rejects_cycles_and_non_folder_parents() {
        let lib = memory_library();
        let p = lib.playlists();
        let outer = p.create("Outer", None, true).unwrap();
        let inner = p.create("Inner", Some(outer), true).unwrap();
        let leaf = p.create("Leaf", Some(inner), false).unwrap();

        assert!(p.move_node(outer, Some(inner), 0).is_err());
        assert!(p.move_node(outer, Some(outer), 0).is_err());
        assert!(p.move_node(inner, Some(leaf), 0).is_err());
        assert!(p.create("Bad", Some(leaf), false).is_err());
        assert_eq!(p.find(inner).unwrap().unwrap().parent_id, Some(outer));
    }

    #[test]
    fn rename_and_failed_move_rolls_back() {
        let lib = memory_library();
        let p = lib.playlists();
        let a = p.create("A", None, false).unwrap();
        let b = p.create("B", None, false).unwrap();
        p.rename(a, "Renamed").unwrap();
        assert!(p.rename(999, "x").is_err());
        assert!(p.move_node(a, Some(999), 0).is_err());
        assert_eq!(names(&p.children(None).unwrap()), ["Renamed", "B"]);
        assert_eq!(p.find(b).unwrap().unwrap().position, 1);
    }
}

mod library_reads {
    use super::*;

    #[test]
    fn failed_reads_are_distinguishable_from_empty() {
        let lib = memory_library();
        assert!(lib.tracks().all().unwrap().is_empty(), "genuinely empty");

        lib.database()
            .exec(
                "DROP TABLE playlist_entries; DROP TABLE cues; DROP TABLE loops; DROP TABLE tracks;",
            )
            .expect("drop tables");
        assert!(lib.tracks().all().is_err());
        assert!(lib.tracks().count().is_err());
        assert!(lib.tracks().find_by_id(1).is_err());
        assert!(lib.cues().cues(1).is_err());
        assert!(lib.playlists().entries(1).is_err());
        assert!(lib.tracks().upsert_by_path(&sample_track("/x")).is_err());
    }
}
