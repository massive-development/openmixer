//! Value types for the search engine: the parsed query, and the SQL fragments fields compile to.
//! Port of `src/library/SearchQuery.h`.

use rusqlite::types::{ToSql, ToSqlOutput};

/// One term of a query. An empty `field` is free text, matched across the text fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterTerm {
    pub field: String,
    pub value: String,
    pub negated: bool,
}

/// A parsed (or hand-built) search. Every term must match (AND).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub terms: Vec<FilterTerm>,
    /// Empty = sort by id.
    pub sort_field: String,
    pub descending: bool,
    /// `None` = no limit.
    pub limit: Option<u32>,
    pub offset: u32,
}

/// A value bound to a `?` placeholder. User text is only ever bound, never spliced into SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchValue {
    Int(i64),
    Real(f64),
    Text(String),
}

impl ToSql for SearchValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Self::Int(v) => v.to_sql(),
            Self::Real(v) => v.to_sql(),
            Self::Text(v) => v.to_sql(),
        }
    }
}

/// SQL text uses only code-constant names and `?` placeholders; user values go in `args`, in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SqlFragment {
    pub sql: String,
    pub args: Vec<SearchValue>,
}
