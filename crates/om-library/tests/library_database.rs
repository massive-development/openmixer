//! Database wrapper, transactions and migrations.

use om_library::{
    core_migrations, current_schema_version, migrate, Database, Library, Migration, OpenMode,
    TrackRecord, Transaction,
};
use std::sync::mpsc;
use std::time::Duration;

const TABLE_A: Migration = Migration {
    version: 1,
    name: "a",
    sql: "CREATE TABLE a (x INTEGER);",
    needs_fts5: false,
};
const TABLE_B: Migration = Migration {
    version: 2,
    name: "b",
    sql: "CREATE TABLE b (x INTEGER);",
    needs_fts5: false,
};

fn memory_db() -> Database {
    Database::open(":memory:", OpenMode::ReadWrite).expect("open :memory:")
}

fn row_count(db: &Database, sql: &str) -> i64 {
    db.connection()
        .query_row(sql, [], |row| row.get(0))
        .expect("count query")
}

fn path_str(dir: &tempfile::TempDir, name: &str) -> String {
    dir.path()
        .join(name)
        .to_str()
        .expect("utf-8 path")
        .to_owned()
}

mod library_migrations {
    use super::*;

    #[test]
    fn empty_database_migrates_to_latest() {
        let db = memory_db();
        assert_eq!(current_schema_version(&db).unwrap(), 0);
        migrate(&db, core_migrations()).expect("migrate");

        let latest = i64::from(core_migrations().last().unwrap().version);
        assert_eq!(current_schema_version(&db).unwrap(), latest);
        assert_eq!(
            row_count(
                &db,
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN \
                 ('tracks','cues','loops','playlists','playlist_entries');"
            ),
            5
        );
    }

    #[test]
    fn rerunning_is_a_no_op() {
        let db = memory_db();
        migrate(&db, core_migrations()).expect("first run");
        migrate(&db, core_migrations()).expect("second run");
        assert_eq!(
            row_count(&db, "SELECT COUNT(*) FROM schema_version;"),
            core_migrations().len() as i64
        );
    }

    #[test]
    fn applies_only_newer_migrations_in_order() {
        let db = memory_db();
        migrate(&db, &[TABLE_A]).expect("v1");
        migrate(&db, &[TABLE_A, TABLE_B]).expect("v2");
        assert_eq!(current_schema_version(&db).unwrap(), 2);
    }

    #[test]
    fn failed_migration_rolls_back_and_keeps_version() {
        let db = memory_db();
        let bad = Migration {
            version: 2,
            name: "bad",
            sql: "CREATE TABLE b (x INTEGER); CREATE TABLE a (y INTEGER);",
            needs_fts5: false,
        };
        assert!(migrate(&db, &[TABLE_A, bad]).is_err());
        assert_eq!(current_schema_version(&db).unwrap(), 1);
        assert_eq!(
            row_count(&db, "SELECT COUNT(*) FROM sqlite_master WHERE name='b';"),
            0
        );
    }

    #[test]
    fn rejects_downgrade_and_unordered_lists() {
        let db = memory_db();
        migrate(&db, &[TABLE_A, TABLE_B]).expect("v2");

        assert!(migrate(&db, &[TABLE_A]).is_err());
        assert!(migrate(&db, &[TABLE_B, TABLE_A]).is_err());
    }
}

mod library_database {
    use super::*;

    #[test]
    fn transaction_rolls_back_unless_committed() {
        let db = memory_db();
        db.exec("CREATE TABLE t (x INTEGER);").unwrap();
        {
            let _tx = Transaction::begin(&db).unwrap();
            db.exec("INSERT INTO t VALUES (1);").unwrap();
        }
        assert_eq!(row_count(&db, "SELECT COUNT(*) FROM t;"), 0);
        {
            let tx = Transaction::begin(&db).unwrap();
            db.exec("INSERT INTO t VALUES (1);").unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(row_count(&db, "SELECT COUNT(*) FROM t;"), 1);
    }

    #[test]
    fn nested_transaction_rolls_back_only_inner_scope() {
        let db = memory_db();
        db.exec("CREATE TABLE t (x INTEGER);").unwrap();
        let outer = Transaction::begin(&db).unwrap();
        db.exec("INSERT INTO t VALUES (1);").unwrap();
        {
            let _inner = Transaction::begin(&db).unwrap();
            db.exec("INSERT INTO t VALUES (2);").unwrap();
        }
        outer.commit().unwrap();
        assert_eq!(row_count(&db, "SELECT COUNT(*) FROM t;"), 1);
    }

    #[test]
    fn bad_sql_yields_invalid_statement_and_error() {
        let db = memory_db();
        assert!(db.prepare("SELEKT 1").is_err());
        let err = db.exec("SELEKT 1").expect_err("bad SQL must fail");
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn file_database_uses_wal_and_allows_concurrent_reader() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = path_str(&dir, "library.db");
        let writer = Library::open(&path, OpenMode::ReadWrite, core_migrations()).expect("writer");
        let track = TrackRecord {
            path: "/music/a.wav".into(),
            ..Default::default()
        };
        writer.tracks().insert(&track).expect("insert");

        let mode: String = writer
            .database()
            .connection()
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");

        let reader = Library::open(&path, OpenMode::ReadOnly, core_migrations()).expect("reader");
        assert_eq!(reader.tracks().count().unwrap(), 1);
        // Read-only rejects writes.
        assert!(reader.tracks().insert(&TrackRecord::default()).is_err());
    }

    #[test]
    fn opening_nonexistent_read_only_file_fails() {
        let result = Library::open(
            "/nonexistent-dir/none.db",
            OpenMode::ReadOnly,
            core_migrations(),
        );
        let err = result.err().expect("must fail");
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn read_error_from_missing_table_surfaces_in_status() {
        let db = memory_db();
        assert!(db.prepare("SELECT * FROM nope;").is_err());
        assert!(db
            .query_rows("SELECT * FROM nope;", [], |_| Ok(()))
            .is_err());
    }

    #[test]
    fn busy_timeout_is_configured() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::open(&path_str(&dir, "busy.db"), OpenMode::ReadWrite).expect("open");
        let timeout: i64 = db
            .connection()
            .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
            .unwrap();
        assert!(timeout > 0);
    }
}

#[test]
fn an_outermost_transaction_waits_for_the_write_lock_instead_of_failing_on_a_stale_snapshot() {
    let dir = tempfile::tempdir().expect("dir");
    let path = path_str(&dir, "contended.db");
    Database::open(&path, OpenMode::ReadWrite)
        .unwrap()
        .exec("CREATE TABLE t (x INTEGER);")
        .unwrap();

    // The writer holds the write lock for a moment.
    let writer = Database::open(&path, OpenMode::ReadWrite).unwrap();
    let held = Transaction::begin(&writer).unwrap();
    writer.exec("INSERT INTO t VALUES (1);").unwrap();

    // The reader reads, then writes. Begun as a deferred savepoint, its read would pin a snapshot
    // before the writer commits, and its write would fail at once with SQLITE_BUSY_SNAPSHOT. Begun
    // with BEGIN IMMEDIATE, it waits for the lock (busy timeout) and reads after the commit.
    let (read_tx, read_rx) = mpsc::channel();
    let reader_path = path.clone();
    let reader = std::thread::spawn(move || -> om_library::Result<i64> {
        let db = Database::open(&reader_path, OpenMode::ReadWrite)?;
        let tx = Transaction::begin(&db)?;
        let seen: i64 = db
            .query_rows("SELECT count(*) FROM t;", [], |row| row.get(0))?
            .remove(0);
        let _ = read_tx.send(());
        db.exec("INSERT INTO t VALUES (2);")?;
        tx.commit()?;
        Ok(seen)
    });
    // Commit once the reader has had its chance to start, or after half a second if it is waiting.
    let _ = read_rx.recv_timeout(Duration::from_millis(500));
    held.commit().unwrap();

    let seen = reader
        .join()
        .expect("reader thread")
        .expect("the reader waits for the lock and then writes");
    assert_eq!(seen, 1, "the reader sees the writer's committed row");
    assert_eq!(row_count(&writer, "SELECT count(*) FROM t;"), 2);
}

#[test]
fn bundled_sqlite_has_fts5_with_unicode61_and_prefix_indexes() {
    let db = memory_db();
    db.exec(
        "CREATE VIRTUAL TABLE probe USING fts5(x, tokenize = 'unicode61 remove_diacritics 2', \
         prefix = '2 3');",
    )
    .expect("FTS5 is compiled into the bundled SQLite");
    db.exec("INSERT INTO probe (x) VALUES ('Émile');")
        .expect("insert");
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM probe WHERE probe MATCH '\"emile\"*';"
        ),
        1,
        "diacritics are folded and the prefix matches"
    );
}

#[test]
fn v7_indexes_existing_rows_of_a_populated_v6_database() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = path_str(&dir, "v6.db");
    {
        let db = Database::open(&path, OpenMode::ReadWrite).expect("open");
        migrate(&db, &core_migrations()[..6]).expect("v6");
        assert_eq!(current_schema_version(&db).unwrap(), 6);
        db.exec(
            "INSERT INTO tracks (path, title, artist, album, genre, comment) VALUES \
             ('/a.wav', 'Émile Hymne', 'Daft', '', 'House', 'late night'), \
             ('/b.wav', 'Love Song', 'Moby', '', 'Techno', '');",
        )
        .expect("populate v6");
    }

    let lib = Library::open(&path, OpenMode::ReadWrite, core_migrations()).expect("upgrade");
    let db = lib.database();
    let latest = i64::from(core_migrations().last().unwrap().version);
    assert_eq!(current_schema_version(db).unwrap(), latest);
    let matches = |q: &str| {
        row_count(
            db,
            &format!("SELECT COUNT(*) FROM tracks_fts WHERE tracks_fts MATCH '{q}';"),
        )
    };
    // The rebuild indexed the rows that existed before the migration, diacritics folded.
    assert_eq!(matches("\"emile\"*"), 1);
    assert_eq!(matches("\"late\"*"), 1);
    assert_eq!(matches("\"moby\"*"), 1);
    // Triggers keep rows written after the upgrade in step.
    lib.tracks()
        .insert(&TrackRecord {
            path: "/c.wav".into(),
            title: "Charlie".into(),
            ..Default::default()
        })
        .expect("insert");
    assert_eq!(matches("\"charlie\"*"), 1);
}
