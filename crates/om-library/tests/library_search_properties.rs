//! Property tests for the search engine: arbitrary query text and hand-built queries must never
//! panic, and must never compile to SQL that SQLite rejects. Each case runs against a fresh empty
//! in-memory library.

use om_library::{
    core_migrations, FilterTerm, Library, LibraryError, OpenMode, SearchEngine, SearchQuery,
};
use proptest::prelude::*;

/// Biased toward the characters the query syntax and the LIKE/SQL builders care about.
const SYNTAX_HEAVY: &str = "[a-z0-9 :\"'%_\\\\=<>~#.\\-/,+*()]{0,48}";

fn empty_library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open :memory:")
}

/// A query error is a legitimate rejection. Anything else (a SQLite error) means the compiler
/// produced SQL the database cannot run.
fn assert_not_invalid_sql<T>(result: om_library::Result<T>, what: &str) {
    if let Err(e) = result {
        assert!(
            matches!(e, LibraryError::Query(_)),
            "{what} produced invalid SQL: {e}"
        );
    }
}

proptest! {
    #[test]
    fn arbitrary_query_text_never_panics_or_yields_invalid_sql(
        text in prop_oneof![any::<String>(), SYNTAX_HEAVY],
    ) {
        let lib = empty_library();
        let engine = SearchEngine::new(lib.database());
        let parsed = engine.parse(&text);
        assert_not_invalid_sql(engine.search(&parsed.query), &text);
        assert_not_invalid_sql(engine.count(&parsed.query), &text);
        assert_not_invalid_sql(engine.search_text(&text), &text);
    }

    #[test]
    fn arbitrary_hand_built_queries_never_yield_invalid_sql(
        terms in proptest::collection::vec(
            (
                prop_oneof![
                    Just(String::new()),
                    Just("title".to_string()),
                    Just("bpm".to_string()),
                    Just("added".to_string()),
                    Just("key".to_string()),
                    Just("playlist".to_string()),
                    Just("folder".to_string()),
                    any::<String>(),
                ],
                prop_oneof![any::<String>(), SYNTAX_HEAVY],
                any::<bool>(),
            ),
            0..4,
        ),
        sort_field in prop_oneof![Just(String::new()), any::<String>()],
        descending in any::<bool>(),
        limit in proptest::option::of(any::<u32>()),
        offset in any::<u32>(),
    ) {
        let lib = empty_library();
        let engine = SearchEngine::new(lib.database());
        let query = SearchQuery {
            terms: terms
                .into_iter()
                .map(|(field, value, negated)| FilterTerm { field, value, negated })
                .collect(),
            sort_field,
            descending,
            limit,
            offset,
        };
        assert_not_invalid_sql(engine.search(&query), "hand-built query");
        assert_not_invalid_sql(engine.count(&query), "hand-built query count");
    }
}
