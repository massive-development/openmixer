//! Key-driven text editing shared by the single-line text fields (the settings overlay's path
//! fields and the debug panel's deck paths). Pure: the caller owns the text and the open state.

/// What a key did to a text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKey {
    /// Enter: the caller applies the text.
    Submit,
    /// Escape: the caller closes the field.
    Cancel,
    /// The text changed (typed or backspaced).
    Edited,
    /// The key does nothing to the field.
    Ignored,
}

/// Applies one key to `text`. `key` is the gpui key name (`enter`, `escape`, `backspace`); `typed`
/// is the text the key produced, if any.
pub fn apply_key(text: &mut String, key: &str, typed: Option<&str>) -> EntryKey {
    match key {
        "enter" => EntryKey::Submit,
        "escape" => EntryKey::Cancel,
        "backspace" => {
            text.pop();
            EntryKey::Edited
        }
        _ => match typed {
            Some(typed) if !typed.is_empty() => {
                text.push_str(typed);
                EntryKey::Edited
            }
            _ => EntryKey::Ignored,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_backspace_enter_and_escape() {
        let mut text = String::from("/music/");
        assert_eq!(apply_key(&mut text, "a", Some("a")), EntryKey::Edited);
        assert_eq!(text, "/music/a");
        assert_eq!(apply_key(&mut text, "backspace", None), EntryKey::Edited);
        assert_eq!(text, "/music/");
        assert_eq!(apply_key(&mut text, "enter", None), EntryKey::Submit);
        assert_eq!(apply_key(&mut text, "escape", None), EntryKey::Cancel);
        assert_eq!(apply_key(&mut text, "shift", None), EntryKey::Ignored);
        assert_eq!(apply_key(&mut text, "x", Some("")), EntryKey::Ignored);
        assert_eq!(text, "/music/");
    }
}
