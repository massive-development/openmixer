//! Small XML tree for the importers: elements with attributes, child elements and their text.
//! Covers the part of `juce::XmlElement` the C++ importers use. Comments, processing
//! instructions and the DOCTYPE are dropped; entity and character references are decoded.

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::Reader;
use std::path::Path;

/// One element of a parsed document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlElement {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<XmlElement>,
    /// Text directly inside this element (not inside its children).
    text: String,
}

impl XmlElement {
    pub fn tag_name(&self) -> &str {
        &self.name
    }

    pub fn has_tag_name(&self, name: &str) -> bool {
        self.name == name
    }

    /// Child elements in document order.
    pub fn children(&self) -> &[XmlElement] {
        &self.children
    }

    /// Child elements with this tag name, in document order.
    pub fn children_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a XmlElement> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// The first child with this tag name.
    pub fn child_by_name(&self, name: &str) -> Option<&XmlElement> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn has_attribute(&self, name: &str) -> bool {
        self.attribute(name).is_some()
    }

    /// The attribute's value, or `None` when absent.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The attribute's value, or "" when absent.
    pub fn string_attribute(&self, name: &str) -> String {
        self.attribute(name).unwrap_or_default().to_owned()
    }

    /// The attribute read as an integer (JUCE rules, see [`crate::juce_text::int_value`]), or
    /// `default` when absent.
    pub fn int_attribute(&self, name: &str, default: i64) -> i64 {
        self.attribute(name)
            .map_or(default, crate::juce_text::int_value)
    }

    /// The attribute read as a number (JUCE rules), or 0.0 when absent.
    pub fn double_attribute(&self, name: &str) -> f64 {
        self.attribute(name)
            .map_or(0.0, crate::juce_text::double_value)
    }

    /// All text in this element and its descendants, in document order.
    pub fn all_text(&self) -> String {
        let mut out = self.text.clone();
        for child in &self.children {
            out.push_str(&child.all_text());
        }
        out
    }
}

/// Parses a whole document and returns its root element.
pub fn parse_xml(text: &str) -> Result<XmlElement, String> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut root: Option<XmlElement> = None;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(e) => stack.push(element_from(&e)?),
            Event::Empty(e) => attach(&mut stack, &mut root, element_from(&e)?),
            Event::End(_) => {
                let done = stack.pop().ok_or_else(|| "unexpected end tag".to_owned())?;
                attach(&mut stack, &mut root, done);
            }
            Event::Text(t) => {
                let decoded = t.decode().map_err(|e| e.to_string())?;
                push_text(&mut stack, &decoded);
            }
            Event::CData(c) => {
                let decoded = c.decode().map_err(|e| e.to_string())?;
                push_text(&mut stack, &decoded);
            }
            Event::GeneralRef(r) => {
                let resolved = resolve_reference(&r)?;
                push_text(&mut stack, &resolved);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("unexpected end of document: unclosed element".to_owned());
    }
    root.ok_or_else(|| "no root element".to_owned())
}

/// Reads and parses a file. Fails when the file cannot be read or does not parse.
pub fn read_xml_file(path: &Path) -> Result<XmlElement, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_xml(&text)
}

fn element_from(start: &BytesStart<'_>) -> Result<XmlElement, String> {
    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|e| e.to_string())?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Explicit1_0)
            .map_err(|e| e.to_string())?;
        attributes.push((key, value.into_owned()));
    }
    Ok(XmlElement {
        name,
        attributes,
        children: Vec::new(),
        text: String::new(),
    })
}

/// Adds a finished element to its parent. Only the first root element is kept; anything after
/// it is ignored, as `juce::XmlDocument` does.
fn attach(stack: &mut [XmlElement], root: &mut Option<XmlElement>, element: XmlElement) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
    } else if root.is_none() {
        *root = Some(element);
    }
}

fn push_text(stack: &mut [XmlElement], text: &str) {
    if let Some(top) = stack.last_mut() {
        top.text.push_str(text);
    }
}

fn resolve_reference(reference: &BytesRef<'_>) -> Result<String, String> {
    if reference.is_char_ref() {
        return match reference.resolve_char_ref() {
            Ok(Some(c)) => Ok(c.to_string()),
            _ => Err("invalid character reference".to_owned()),
        };
    }
    let name = reference.decode().map_err(|e| e.to_string())?;
    Ok(match name.as_ref() {
        "amp" => "&".to_owned(),
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "quot" => "\"".to_owned(),
        "apos" => "'".to_owned(),
        other => format!("&{other};"),
    })
}
