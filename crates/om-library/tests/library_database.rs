//! Port of tests/unit/LibraryDatabaseTest.cpp: database wrapper, transactions and migrations.

use om_library::{
    core_migrations, current_schema_version, migrate, Database, Library, Migration, OpenMode,
    TrackRecord, Transaction,
};

const TABLE_A: Migration = Migration {
    version: 1,
    name: "a",
    sql: "CREATE TABLE a (x INTEGER);",
};
const TABLE_B: Migration = Migration {
    version: 2,
    name: "b",
    sql: "CREATE TABLE b (x INTEGER);",
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
