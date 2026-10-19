//! Runs searches against a library database.
//! Threading: use on the thread that owns the `Database` (the UI gets its own read-only connection).

use crate::database::{Database, LibraryCapabilities};
use crate::error::{LibraryError, Result};
use crate::search_compiler::{compile_search, CompileContext, CompiledSearch};
use crate::search_fields::SearchFieldRegistry;
use crate::search_parser::{parse_query, ParsedQuery};
use crate::search_query::SearchQuery;
use crate::track_dao::read_row;
use crate::track_record::TrackRecord;
use crate::watch_folders::Clock;
use rusqlite::params_from_iter;
use std::borrow::Borrow;
use std::sync::Arc;

pub struct SearchEngine<D: Borrow<Database>> {
    db: D,
    fields: SearchFieldRegistry,
    /// Clock for relative dates. Tests inject a fixed one.
    pub clock: Clock,
    /// Detected when the engine is created. Set `fts5` to false to force LIKE free-text search.
    pub capabilities: LibraryCapabilities,
}

impl<D: Borrow<Database>> SearchEngine<D> {
    /// An engine over `db` with the built-in fields and the system clock.
    pub fn new(db: D) -> Self {
        Self::with_registry(db, SearchFieldRegistry::with_defaults())
    }

    pub fn with_registry(db: D, fields: SearchFieldRegistry) -> Self {
        let capabilities = LibraryCapabilities::detect(db.borrow());
        Self {
            db,
            fields,
            clock: Arc::new(system_now_ms),
            capabilities,
        }
    }

    pub fn registry(&self) -> &SearchFieldRegistry {
        &self.fields
    }

    /// For registering plugin fields.
    pub fn registry_mut(&mut self) -> &mut SearchFieldRegistry {
        &mut self.fields
    }

    pub fn parse(&self, text: &str) -> ParsedQuery {
        parse_query(text, &self.fields)
    }

    /// Runs a parsed query. A failed compile or a failed read is an error, never an empty result,
    /// so a partial page is never presented as the answer.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<TrackRecord>> {
        self.run(query, |compiled| {
            self.db.borrow().query_rows(
                &compiled.select_sql,
                params_from_iter(compiled.select_args.iter()),
                read_row,
            )
        })
    }

    /// Parses and runs query text. Parse errors (bad sort, limit or offset) fail the search.
    pub fn search_text(&self, text: &str) -> Result<Vec<TrackRecord>> {
        let parsed = self.parse(text);
        if !parsed.errors.is_empty() {
            return Err(LibraryError::Query(parsed.errors.join("; ")));
        }
        self.search(&parsed.query)
    }

    /// Number of matches. Ignores limit and offset.
    pub fn count(&self, query: &SearchQuery) -> Result<i64> {
        self.run(query, |compiled| {
            let rows = self.db.borrow().query_rows(
                &compiled.count_sql,
                params_from_iter(compiled.count_args.iter()),
                |row| row.get::<_, i64>(0),
            )?;
            Ok(rows.into_iter().next().unwrap_or(0))
        })
    }

    /// Compiles with FTS5 when the capability says so, then runs. If the FTS index turns out to be
    /// missing at run time, the query is compiled again with LIKE and run once more.
    fn run<T>(
        &self,
        query: &SearchQuery,
        exec: impl Fn(&CompiledSearch) -> Result<T>,
    ) -> Result<T> {
        let ctx = self.context();
        let compiled = compile_search(query, &self.fields, &ctx)?;
        match exec(&compiled) {
            Err(e) if ctx.fts5 && is_missing_fts(&e) => {
                let ctx = CompileContext { fts5: false, ..ctx };
                exec(&compile_search(query, &self.fields, &ctx)?)
            }
            result => result,
        }
    }

    fn context(&self) -> CompileContext {
        CompileContext {
            now_ms: (self.clock)(),
            fts5: self.capabilities.fts5,
        }
    }
}

/// The error SQLite gives when the FTS5 module or the `tracks_fts` table is absent.
fn is_missing_fts(e: &LibraryError) -> bool {
    match e {
        LibraryError::Sqlite(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
            msg.contains("no such module: fts5") || msg.contains("no such table: tracks_fts")
        }
        _ => false,
    }
}

fn system_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
