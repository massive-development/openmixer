//! Field registry for search: maps a query field name ("bpm", "artist") to how it filters and sorts.
//! Extension point: plugins add fields with a custom compile hook.

use crate::error::Result;
use crate::search_compiler::{compile_folder_term, compile_key_term, compile_playlist_term};
use crate::search_query::{FilterTerm, SqlFragment};
use std::sync::Arc;

/// How a field's values are read: text (LIKE / `=`), number (ranges), date (day ranges, relative
/// `7d`), or custom (the field's own compile hook).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
    Date,
    Custom,
}

/// Custom-field compile hook: append SQL for `term` to `out`. Values go in `out.args`; never splice
/// user text into `out.sql`.
pub type CustomCompile = Arc<dyn Fn(&FilterTerm, &mut SqlFragment) -> Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct SearchField {
    /// Lower-case token used in query strings.
    pub name: String,
    pub kind: FieldKind,
    /// Code-constant SQL expression, for example "tracks.bpm". Unused for custom fields.
    pub column: String,
    /// Code-constant ORDER BY expressions; empty = not sortable.
    pub sort_columns: Vec<String>,
    /// Custom fields only.
    pub compile: Option<CustomCompile>,
}

impl SearchField {
    /// A column-backed field. Text fields sort case-insensitively.
    fn simple(name: &str, kind: FieldKind, column: &str, nocase_sort: bool) -> Self {
        let sort = if nocase_sort {
            format!("{column} COLLATE NOCASE")
        } else {
            column.to_string()
        };
        Self {
            name: name.to_string(),
            kind,
            column: column.to_string(),
            sort_columns: vec![sort],
            compile: None,
        }
    }

    /// A field with its own compile hook (for plugins as well as the built-in key, playlist and
    /// folder fields).
    pub fn custom(
        name: &str,
        sort_by: &[&str],
        compile: impl Fn(&FilterTerm, &mut SqlFragment) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.to_string(),
            kind: FieldKind::Custom,
            column: String::new(),
            sort_columns: sort_by.iter().map(|s| s.to_string()).collect(),
            compile: Some(Arc::new(compile)),
        }
    }
}

/// The registry of fields a query may name.
#[derive(Clone, Default)]
pub struct SearchFieldRegistry {
    fields: Vec<SearchField>,
}

impl SearchFieldRegistry {
    /// Built-ins: title artist album genre comment path year bpm rating plays duration added played
    /// key playlist folder.
    pub fn with_defaults() -> Self {
        let mut r = Self::default();
        for name in ["title", "artist", "album", "genre", "comment", "path"] {
            r.add(SearchField::simple(
                name,
                FieldKind::Text,
                &format!("tracks.{name}"),
                true,
            ));
        }
        r.add(SearchField::simple(
            "year",
            FieldKind::Number,
            "tracks.year",
            false,
        ));
        r.add(SearchField::simple(
            "bpm",
            FieldKind::Number,
            "tracks.bpm",
            false,
        ));
        r.add(SearchField::simple(
            "rating",
            FieldKind::Number,
            "tracks.rating",
            false,
        ));
        r.add(SearchField::simple(
            "plays",
            FieldKind::Number,
            "tracks.play_count",
            false,
        ));
        r.add(SearchField::simple(
            "duration",
            FieldKind::Number,
            "tracks.duration_sec",
            false,
        ));
        r.add(SearchField::simple(
            "added",
            FieldKind::Date,
            "tracks.date_added_ms",
            false,
        ));
        r.add(SearchField::simple(
            "played",
            FieldKind::Date,
            "tracks.last_played_ms",
            false,
        ));
        r.add(SearchField::custom(
            "key",
            &["CAST(tracks.camelot AS INTEGER)", "tracks.camelot"],
            compile_key_term,
        ));
        r.add(SearchField::custom("playlist", &[], compile_playlist_term));
        r.add(SearchField::custom("folder", &[], compile_folder_term));
        r
    }

    /// Adds a field, replacing any field of the same name. Names are stored lower-case.
    pub fn add(&mut self, mut field: SearchField) {
        field.name = field.name.to_lowercase();
        match self.fields.iter_mut().find(|f| f.name == field.name) {
            Some(existing) => *existing = field,
            None => self.fields.push(field),
        }
    }

    /// Case-insensitive lookup.
    pub fn find(&self, name: &str) -> Option<&SearchField> {
        let key = name.to_lowercase();
        self.fields.iter().find(|f| f.name == key)
    }

    pub fn names(&self) -> Vec<String> {
        self.fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// The text columns that free-text terms search.
pub fn free_text_columns() -> &'static [&'static str] {
    &[
        "tracks.title",
        "tracks.artist",
        "tracks.album",
        "tracks.genre",
        "tracks.comment",
    ]
}
