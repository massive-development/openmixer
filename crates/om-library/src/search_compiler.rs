//! Compiles a `SearchQuery` to parameterised SQL over the `tracks` table, and the per-term compilers
//! for the built-in field kinds. Column names come from the registry (code constants); every user
//! value is a bound argument.

use crate::error::{LibraryError, Result};
use crate::search_fields::{free_text_columns, FieldKind, SearchField, SearchFieldRegistry};
use crate::search_keys::key_camelot_codes;
use crate::search_query::{FilterTerm, SearchQuery, SearchValue, SqlFragment};
use crate::track_dao::SELECT_COLUMNS;

const DAY_MS: i64 = 86_400_000;

/// A compiled search: the page query (ends with `LIMIT ? OFFSET ?`) and the count query, which
/// ignores limit and offset.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledSearch {
    pub select_sql: String,
    pub count_sql: String,
    pub select_args: Vec<SearchValue>,
    pub count_args: Vec<SearchValue>,
}

/// Per-compile inputs. `now_ms` anchors relative dates such as `added:7d`. `fts5` selects the FTS5
/// index for free text; `false` compiles free text to LIKE (the default, so plain compiles need no
/// database).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileContext {
    pub now_ms: i64,
    pub fts5: bool,
}

/// Compiles `query` against `fields`. Fails on an unknown field, an unreadable value or an
/// unsortable sort field; no partial SQL is returned.
pub fn compile_search(
    query: &SearchQuery,
    fields: &SearchFieldRegistry,
    ctx: &CompileContext,
) -> Result<CompiledSearch> {
    let where_ = compile_where(query, fields, ctx)?;
    let order = compile_order(query, fields)?;

    let count_sql = format!("SELECT COUNT(*) FROM tracks{}", where_.sql);
    let select_sql = format!(
        "SELECT {SELECT_COLUMNS} FROM tracks{}{order} LIMIT ? OFFSET ?",
        where_.sql
    );

    let mut select_args = where_.args.clone();
    select_args.push(SearchValue::Int(query.limit.map_or(-1, i64::from)));
    select_args.push(SearchValue::Int(i64::from(query.offset)));

    Ok(CompiledSearch {
        select_sql,
        count_sql,
        select_args,
        count_args: where_.args,
    })
}

/// `%text%` with LIKE wildcards in `text` escaped. Pair with `ESCAPE '\'`.
pub fn like_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Appends `<column> = ? COLLATE NOCASE` (value starts with '=') or `<column> LIKE ? ESCAPE '\'`
/// (contains) to `out`.
pub fn append_text_predicate(column: &str, value: &str, out: &mut SqlFragment) {
    if let Some(exact) = value.strip_prefix('=') {
        out.sql.push_str(&format!("{column} = ? COLLATE NOCASE"));
        out.args.push(SearchValue::Text(exact.to_string()));
    } else {
        out.sql.push_str(&format!("{column} LIKE ? ESCAPE '\\'"));
        out.args.push(SearchValue::Text(like_pattern(value)));
    }
}

fn compile_term(
    term: &FilterTerm,
    fields: &SearchFieldRegistry,
    ctx: &CompileContext,
    out: &mut SqlFragment,
) -> Result<()> {
    if term.field.is_empty() {
        return compile_free_text(term, ctx.fts5, out);
    }
    let field = fields
        .find(&term.field)
        .ok_or_else(|| LibraryError::Query(format!("unknown field '{}'", term.field)))?;
    match field.kind {
        FieldKind::Text => compile_text_term(field, term, out),
        FieldKind::Custom => match &field.compile {
            Some(compile) => compile(term, out),
            None => Err(LibraryError::Query(format!("{}: no compiler", field.name))),
        },
        FieldKind::Number | FieldKind::Date => compile_range_term(field, term, ctx.now_ms, out),
    }
}

/// ` WHERE (term) AND (term) ...`, or empty when there are no terms.
fn compile_where(
    query: &SearchQuery,
    fields: &SearchFieldRegistry,
    ctx: &CompileContext,
) -> Result<SqlFragment> {
    let mut where_ = SqlFragment::default();
    for term in &query.terms {
        let mut part = SqlFragment::default();
        compile_term(term, fields, ctx, &mut part)?;
        where_.sql.push_str(if where_.sql.is_empty() {
            " WHERE ("
        } else {
            " AND ("
        });
        where_.sql.push_str(&part.sql);
        where_.sql.push(')');
        where_.args.extend(part.args);
    }
    Ok(where_)
}

fn compile_order(query: &SearchQuery, fields: &SearchFieldRegistry) -> Result<String> {
    let mut sql = " ORDER BY ".to_string();
    if !query.sort_field.is_empty() {
        let field = fields
            .find(&query.sort_field)
            .filter(|f| !f.sort_columns.is_empty())
            .ok_or_else(|| LibraryError::Query(format!("cannot sort by '{}'", query.sort_field)))?;
        for column in &field.sort_columns {
            sql.push_str(column);
            sql.push_str(if query.descending {
                " DESC, "
            } else {
                " ASC, "
            });
        }
    }
    sql.push_str("tracks.id ASC"); // stable paging
    Ok(sql)
}

/// Free text. With FTS5 it is a token-prefix query over the `tracks_fts` index (see [`fts_query`]);
/// text with no letters or digits has no tokens, so it keeps the LIKE form. The LIKE form matches
/// substrings of each text column.
fn compile_free_text(term: &FilterTerm, fts5: bool, out: &mut SqlFragment) -> Result<()> {
    if fts5 {
        if let Some(query) = fts_query(&term.value) {
            out.sql.push_str(if term.negated {
                "tracks.id NOT IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?)"
            } else {
                "tracks.id IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?)"
            });
            out.args.push(SearchValue::Text(query));
            return Ok(());
        }
    }
    let mut f = SqlFragment::default();
    for column in free_text_columns() {
        if !f.sql.is_empty() {
            f.sql.push_str(" OR ");
        }
        f.sql.push_str(&format!("{column} LIKE ? ESCAPE '\\'"));
        f.args.push(SearchValue::Text(like_pattern(&term.value)));
    }
    out.sql.push_str(if term.negated { "NOT (" } else { "(" });
    out.sql.push_str(&f.sql);
    out.sql.push(')');
    out.args.extend(f.args);
    Ok(())
}

/// Converts free text into an FTS5 `MATCH` argument that cannot be read as query syntax. Only
/// letters and digits survive, each run becomes a token in double quotes, so quotes, `*`, `NEAR`,
/// `OR`, parentheses and column names are plain text. One run of text is one phrase, and its last
/// token is a prefix (`"deep house"*`). `None` when there are no letters or digits.
pub fn fts_query(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    Some(format!("\"{}\"*", tokens.join(" ")))
}

fn compile_text_term(field: &SearchField, term: &FilterTerm, out: &mut SqlFragment) -> Result<()> {
    let mut f = SqlFragment::default();
    append_text_predicate(&field.column, &term.value, &mut f);
    if term.negated {
        out.sql.push_str(&format!("NOT ({})", f.sql));
    } else {
        out.sql.push_str(&f.sql);
    }
    out.args.extend(f.args);
    Ok(())
}

/// A numeric or date interval, half-open at either end. `lo_op` / `hi_op` are the SQL operators.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Interval {
    lo: Option<f64>,
    hi: Option<f64>,
    lo_op: &'static str,
    hi_op: &'static str,
}

/// One value as an interval. Dates span their whole day (UTC); numbers are exact.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Endpoint {
    start: f64,
    end: f64,
    closed_end: bool,
}

fn compile_range_term(
    field: &SearchField,
    term: &FilterTerm,
    now_ms: i64,
    out: &mut SqlFragment,
) -> Result<()> {
    let iv = parse_interval(field.kind, &term.value, now_ms).ok_or_else(|| {
        LibraryError::Query(format!("{}: cannot read '{}'", field.name, term.value))
    })?;

    let mut f = SqlFragment::default();
    if let Some(lo) = iv.lo {
        f.sql.push_str(&format!("{} {} ?", field.column, iv.lo_op));
        f.args.push(bound_value(field.kind, lo));
    }
    if iv.lo.is_some() && iv.hi.is_some() {
        f.sql.push_str(" AND ");
    }
    if let Some(hi) = iv.hi {
        f.sql.push_str(&format!("{} {} ?", field.column, iv.hi_op));
        f.args.push(bound_value(field.kind, hi));
    }

    out.sql.push('(');
    out.sql.push_str(&wrap_negation(&f.sql, term.negated));
    out.sql.push(')');
    out.args.extend(f.args);
    Ok(())
}

/// Negated ranges keep rows where the column is NULL (the COALESCE to 0 makes the predicate false).
fn wrap_negation(predicate: &str, negated: bool) -> String {
    if negated {
        format!("NOT COALESCE({predicate}, 0)")
    } else {
        predicate.to_string()
    }
}

fn bound_value(kind: FieldKind, v: f64) -> SearchValue {
    if kind == FieldKind::Date {
        SearchValue::Int(v as i64)
    } else {
        SearchValue::Real(v)
    }
}

pub fn compile_key_term(term: &FilterTerm, out: &mut SqlFragment) -> Result<()> {
    let compatible = term.value.starts_with('~');
    let text = if compatible {
        &term.value[1..]
    } else {
        &term.value
    };
    let codes = key_camelot_codes(text, compatible);
    if codes.is_empty() {
        return Err(LibraryError::Query(format!(
            "key: unknown key '{}'",
            term.value
        )));
    }

    out.sql.push_str(&format!(
        "tracks.camelot {}IN (",
        if term.negated { "NOT " } else { "" }
    ));
    for (i, code) in codes.into_iter().enumerate() {
        out.sql.push_str(if i == 0 { "?" } else { ",?" });
        out.args.push(SearchValue::Text(code));
    }
    out.sql.push(')');
    Ok(())
}

pub fn compile_playlist_term(term: &FilterTerm, out: &mut SqlFragment) -> Result<()> {
    let mut f = SqlFragment::default();
    if let Some(id) = term.value.strip_prefix('#') {
        if id.is_empty() || id.len() > 15 || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(LibraryError::Query(format!(
                "playlist: bad id '{}'",
                term.value
            )));
        }
        let id: i64 = id
            .parse()
            .map_err(|_| LibraryError::Query(format!("playlist: bad id '{}'", term.value)))?;
        f.sql = "tracks.id IN (SELECT track_id FROM playlist_entries WHERE playlist_id = ?)"
            .to_string();
        f.args.push(SearchValue::Int(id));
    } else {
        f.sql = "tracks.id IN (SELECT e.track_id FROM playlist_entries e \
                 JOIN playlists p ON p.id = e.playlist_id WHERE "
            .to_string();
        append_text_predicate("p.name", &term.value, &mut f);
        f.sql.push(')');
    }
    if term.negated {
        out.sql.push_str(&format!("NOT ({})", f.sql));
    } else {
        out.sql.push_str(&f.sql);
    }
    out.args.extend(f.args);
    Ok(())
}

/// `folder:<dir>` matches tracks under `dir`: anchored at the start and followed by a separator, so
/// siblings that share a name prefix are not matched.
pub fn compile_folder_term(term: &FilterTerm, out: &mut SqlFragment) -> Result<()> {
    let mut dir = term.value.as_str();
    while dir.len() > 1 && (dir.ends_with('/') || dir.ends_with('\\')) {
        dir = &dir[..dir.len() - 1];
    }
    if dir.is_empty() {
        return Err(LibraryError::Query("folder: empty path".to_string()));
    }

    let pattern = like_pattern(dir);
    // Strip the wildcards that like_pattern adds around the text.
    let escaped = &pattern[1..pattern.len() - 1];
    let root = dir == "/"; // "/" + "/%" would never match
    let (sep_a, sep_b) = if root { ("%", "%") } else { ("/%", "\\\\%") };

    out.sql.push_str(if term.negated { "NOT (" } else { "(" });
    out.sql
        .push_str("tracks.path LIKE ? ESCAPE '\\' OR tracks.path LIKE ? ESCAPE '\\')");
    out.args
        .push(SearchValue::Text(format!("{escaped}{sep_a}")));
    out.args
        .push(SearchValue::Text(format!("{escaped}{sep_b}")));
    Ok(())
}

fn parse_number(s: &str) -> Option<f64> {
    let dots = s.bytes().filter(|&b| b == b'.').count();
    let well_formed = !s.is_empty()
        && s.len() <= 15
        && dots <= 1
        && s != "."
        && s.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    if !well_formed {
        return None;
    }
    // Rust accepts "5." and ".5" as floats, so both forms parse as numbers.
    s.parse().ok()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's civil-to-days algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `yyyy-mm-dd`, read as the UTC day it names.
fn parse_date(s: &str) -> Option<Endpoint> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || !s.bytes().all(|b| b.is_ascii_digit() || b == b'-')
    {
        return None;
    }
    let num = |p: &str| p.parse::<i64>().unwrap_or(0);
    let (y, m, d) = (num(parts[0]), num(parts[1]), num(parts[2]));
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let start = days_from_civil(y, m, d) * DAY_MS;
    Some(Endpoint {
        start: start as f64,
        end: (start + DAY_MS) as f64,
        closed_end: false,
    })
}

fn parse_endpoint(kind: FieldKind, s: &str) -> Option<Endpoint> {
    if kind == FieldKind::Date {
        return parse_date(s);
    }
    parse_number(s).map(|n| Endpoint {
        start: n,
        end: n,
        closed_end: true,
    })
}

fn hi_op_for(e: &Endpoint) -> &'static str {
    if e.closed_end {
        "<="
    } else {
        "<"
    }
}

fn interval_from_range(kind: FieldKind, a: &str, b: &str) -> Option<Interval> {
    let lo = parse_endpoint(kind, a)?;
    let hi = parse_endpoint(kind, b)?;
    Some(Interval {
        lo: Some(lo.start),
        hi: Some(hi.end),
        lo_op: ">=",
        hi_op: hi_op_for(&hi),
    })
}

fn interval_from_comparison(kind: FieldKind, op: &str, v: &str) -> Option<Interval> {
    let e = parse_endpoint(kind, v)?;
    let iv = match op {
        ">" => Interval {
            lo: Some(e.end),
            hi: None,
            lo_op: if e.closed_end { ">" } else { ">=" },
            hi_op: "<=",
        },
        ">=" => Interval {
            lo: Some(e.start),
            hi: None,
            lo_op: ">=",
            hi_op: "<=",
        },
        "<" => Interval {
            lo: None,
            hi: Some(e.start),
            lo_op: ">=",
            hi_op: "<",
        },
        "<=" => Interval {
            lo: None,
            hi: Some(e.end),
            lo_op: ">=",
            hi_op: hi_op_for(&e),
        },
        // "=" or a bare value: the whole endpoint.
        _ => Interval {
            lo: Some(e.start),
            hi: Some(e.end),
            lo_op: ">=",
            hi_op: hi_op_for(&e),
        },
    };
    Some(iv)
}

/// `Nd`: the last N days up to `now_ms`.
fn interval_from_relative(v: &str, now_ms: i64) -> Option<Interval> {
    let digits = v.strip_suffix('d')?;
    if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let days: i64 = digits.parse().ok()?;
    Some(Interval {
        lo: Some((now_ms - days * DAY_MS) as f64),
        hi: None,
        lo_op: ">=",
        hi_op: "<=",
    })
}

fn parse_interval(kind: FieldKind, v: &str, now_ms: i64) -> Option<Interval> {
    if kind == FieldKind::Date {
        if let Some(rel) = interval_from_relative(v, now_ms) {
            return Some(rel);
        }
    }
    if let Some(dots) = v.find("..") {
        if dots > 0 {
            return interval_from_range(kind, &v[..dots], &v[dots + 2..]);
        }
    }
    if kind == FieldKind::Number {
        if let Some(dash) = v.find('-') {
            if dash > 0 {
                return interval_from_range(kind, &v[..dash], &v[dash + 1..]);
            }
        }
    }

    // The first operator that prefixes the value; order matters (">=" before ">").
    let op = [">=", "<=", ">", "<", "="]
        .into_iter()
        .find(|candidate| v.starts_with(candidate))
        .unwrap_or("");
    interval_from_comparison(kind, op, &v[op.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards_and_backslash() {
        assert_eq!(like_pattern("a%_\\b"), "%a\\%\\_\\\\b%");
    }

    #[test]
    fn fts_query_quotes_every_token_and_prefixes_the_phrase() {
        assert_eq!(fts_query("emile").as_deref(), Some("\"emile\"*"));
        assert_eq!(fts_query("deep house").as_deref(), Some("\"deep house\"*"));
        assert_eq!(fts_query("  Émile!! ").as_deref(), Some("\"Émile\"*"));
        assert_eq!(fts_query("NEAR(a b) OR \"x*^").as_deref(), Some("\"NEAR a b OR x\"*"));
        assert_eq!(fts_query("'; DROP TABLE tracks; --").as_deref(), Some("\"DROP TABLE tracks\"*"));
        assert_eq!(fts_query(")(*\"-"), None);
        assert_eq!(fts_query(""), None);
    }

    #[test]
    fn days_from_civil_matches_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 3, 1) * DAY_MS, 1_709_251_200_000);
    }

    #[test]
    fn number_parsing_follows_the_c_rules() {
        assert_eq!(parse_number("124.5"), Some(124.5));
        assert_eq!(parse_number("5."), Some(5.0));
        assert_eq!(parse_number(".5"), Some(0.5));
        assert_eq!(parse_number("."), None);
        assert_eq!(parse_number("1.2.3"), None);
        assert_eq!(parse_number("-1"), None);
        assert_eq!(parse_number(""), None);
    }
}
