//! Typed access to an Apple property-list `<dict>` (alternating `<key>` and value elements).
//! Port of `src/library/PlistReader.*`.

use crate::juce_text::iso8601_ms;
use crate::xml::XmlElement;
use std::collections::BTreeMap;

/// A property-list dictionary. Borrows the element it was built from.
pub struct PlistDict<'a> {
    values: BTreeMap<String, &'a XmlElement>,
    ordered: Vec<(String, &'a XmlElement)>,
}

impl<'a> PlistDict<'a> {
    pub fn new(dict: &'a XmlElement) -> Self {
        let mut values = BTreeMap::new();
        let mut ordered = Vec::new();
        let mut pending_key: Option<String> = None;
        for child in dict.children() {
            if child.has_tag_name("key") {
                pending_key = Some(child.all_text());
            } else if let Some(key) = pending_key.take() {
                values.insert(key.clone(), child);
                ordered.push((key, child));
            }
        }
        Self { values, ordered }
    }

    pub fn has(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    pub fn find(&self, key: &str) -> Option<&'a XmlElement> {
        self.values.get(key).copied()
    }

    pub fn string(&self, key: &str) -> String {
        self.find(key).map(XmlElement::all_text).unwrap_or_default()
    }

    /// The `<integer>` value, or `fallback` when the key is absent or not an integer.
    pub fn integer(&self, key: &str, fallback: i64) -> i64 {
        match self.find(key) {
            Some(e) if e.has_tag_name("integer") => {
                crate::juce_text::int_value(e.all_text().trim())
            }
            _ => fallback,
        }
    }

    /// True only for a `<true/>` value.
    pub fn boolean(&self, key: &str) -> bool {
        self.find(key).is_some_and(|e| e.has_tag_name("true"))
    }

    /// The ISO-8601 `<date>` in milliseconds since the epoch, 0 when absent.
    pub fn date_ms(&self, key: &str) -> i64 {
        let text = self.string(key);
        if text.is_empty() {
            0
        } else {
            iso8601_ms(&text)
        }
    }

    /// Every (key, value element) pair in document order.
    pub fn entries(&self) -> &[(String, &'a XmlElement)] {
        &self.ordered
    }
}
