//! Query-string parser: `artist:foo bpm:120-128 key:~8A "deep house" -genre:techno sort:-bpm limit:50`.
//! Port of `src/library/SearchParser.{h,cpp}`.

use crate::search_fields::SearchFieldRegistry;
use crate::search_query::{FilterTerm, SearchQuery};

/// A parsed query plus the reserved-word errors. The rest of the query stays usable when errors are
/// present; the engine refuses to run a query that has any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub query: SearchQuery,
    /// Bad sort/limit/offset values.
    pub errors: Vec<String>,
}

/// `name:value` is a filter only when `name` is a registered field (so "12:30" stays free text).
/// Reserved: `sort:<field>` (prefix '-' for descending), `limit:<n>`, `offset:<n>`. Quotes group
/// words; a leading '-' negates.
pub fn parse_query(text: &str, fields: &SearchFieldRegistry) -> ParsedQuery {
    let mut out = ParsedQuery::default();
    for tok in tokenize(text) {
        if !tok.text.is_empty() {
            add_token(&tok, fields, &mut out);
        }
    }
    out
}

#[derive(Debug)]
struct RawToken {
    text: String,
    negated: bool,
}

#[derive(Default)]
struct Scanner {
    out: Vec<RawToken>,
    current: String,
    negated: bool,
    started: bool,
}

impl Scanner {
    fn flush(&mut self) {
        if self.started && (!self.current.is_empty() || self.negated) {
            self.out.push(RawToken {
                text: std::mem::take(&mut self.current),
                negated: self.negated,
            });
        }
        self.current.clear();
        self.negated = false;
        self.started = false;
    }
}

/// Splits on whitespace outside double quotes. A '-' before anything else on the token negates it.
fn tokenize(text: &str) -> Vec<RawToken> {
    let mut s = Scanner::default();
    let mut quoted = false;
    for c in text.chars() {
        if c == '"' {
            quoted = !quoted;
            s.started = true;
            continue;
        }
        if !quoted && c.is_whitespace() {
            s.flush();
            continue;
        }
        if !s.started && c == '-' {
            s.negated = true;
            s.started = true;
            continue;
        }
        s.started = true;
        s.current.push(c);
    }
    s.flush();
    s.out
}

fn parse_count(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 9 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn apply_reserved(name: &str, value: &str, q: &mut SearchQuery, errors: &mut Vec<String>) {
    if name == "sort" {
        q.descending = value.starts_with('-');
        let field = if q.descending { &value[1..] } else { value };
        q.sort_field = field.to_lowercase();
        if q.sort_field.is_empty() {
            errors.push("sort: expected a field name".to_string());
        }
        return;
    }
    match parse_count(value) {
        None => errors.push(format!(
            "{name}: expected a non-negative number, got '{value}'"
        )),
        Some(n) if name == "limit" => q.limit = Some(n),
        Some(n) => q.offset = n,
    }
}

fn is_reserved(name: &str) -> bool {
    matches!(name, "sort" | "limit" | "offset")
}

fn add_token(tok: &RawToken, fields: &SearchFieldRegistry, out: &mut ParsedQuery) {
    let (name, value) = match tok.text.find(':') {
        Some(colon) if colon > 0 => (tok.text[..colon].to_lowercase(), &tok.text[colon + 1..]),
        _ => (String::new(), tok.text.as_str()),
    };

    if is_reserved(&name) && !tok.negated {
        apply_reserved(&name, value, &mut out.query, &mut out.errors);
    } else if fields.find(&name).is_some() {
        out.query.terms.push(FilterTerm {
            field: name,
            value: value.to_string(),
            negated: tok.negated,
        });
    } else {
        out.query.terms.push(FilterTerm {
            field: String::new(),
            value: tok.text.clone(),
            negated: tok.negated,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_name_never_matches_a_field() {
        let fields = SearchFieldRegistry::with_defaults();
        let p = parse_query(":x", &fields);
        assert_eq!(p.query.terms[0].field, "");
        assert_eq!(p.query.terms[0].value, ":x");
    }

    #[test]
    fn negated_reserved_word_is_free_text() {
        let fields = SearchFieldRegistry::with_defaults();
        let p = parse_query("-sort:bpm", &fields);
        assert!(p.query.sort_field.is_empty());
        assert_eq!(p.query.terms[0].value, "sort:bpm");
        assert!(p.query.terms[0].negated);
    }
}
