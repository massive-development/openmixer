//! Turns the `file://` URLs found in iTunes and Rekordbox exports into local paths.

/// Handles `file:///p`, `file://localhost/p`, drive letters (`file:///C:/p`), UNC hosts and `%XX`
/// (UTF-8). `None` when the text is not a file URL.
pub fn file_url_to_path(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let rest = strip_prefix_ignore_case(trimmed, "file://")?;
    let (host, raw_path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, ""),
    };
    let path = percent_decode(raw_path);
    if path.is_empty() {
        return None;
    }
    let path = if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        format!("//{host}{path}") // UNC share
    } else if has_drive_letter(&path[1..]) {
        path[1..].to_owned() // "/C:/x" -> "C:/x"
    } else {
        path
    };
    Some(native_separators(path))
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// Not a generic URL decoder: '+' stays '+', which is valid in file names.
fn percent_decode(text: &str) -> String {
    let input = text.as_bytes();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let decoded = if input[i] == b'%' && i + 2 < input.len() {
            hex_value(input[i + 1]).zip(hex_value(input[i + 2]))
        } else {
            None
        };
        match decoded {
            Some((hi, lo)) => {
                out.push(hi * 16 + lo);
                i += 3;
            }
            None => {
                out.push(input[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn has_drive_letter(path: &str) -> bool {
    let mut chars = path.chars();
    matches!((chars.next(), chars.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic())
}

#[cfg(windows)]
fn native_separators(path: String) -> String {
    path.replace('/', "\\")
}

#[cfg(not(windows))]
fn native_separators(path: String) -> String {
    path
}

#[cfg(test)]
mod tests {
    use super::file_url_to_path;

    // The function returns native separators (`\` on Windows), so expectations are written with `/`
    // and converted the same way the production code does.
    fn native(path: &str) -> String {
        if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path.to_owned()
        }
    }

    // Expectations from tests/unit/LibraryImportTest.cpp (ImportFileUrl.DecodesCommonForms).
    #[test]
    fn decodes_common_forms() {
        assert_eq!(
            file_url_to_path("file:///music/a%20b.mp3"),
            Some(native("/music/a b.mp3"))
        );
        assert_eq!(
            file_url_to_path("file://localhost/music/x.wav"),
            Some(native("/music/x.wav"))
        );
        assert_eq!(
            file_url_to_path("FILE://localhost/music/x.wav"),
            Some(native("/music/x.wav"))
        );
        assert_eq!(
            file_url_to_path("file:///music/Caf%C3%A9+Mix%25.wav"),
            Some(native("/music/Café+Mix%.wav"))
        );
    }

    #[test]
    fn rejects_non_file_urls_and_empty_paths() {
        assert_eq!(file_url_to_path("https://example.com/a.mp3"), None);
        assert_eq!(file_url_to_path("http://example.invalid/a"), None);
        assert_eq!(file_url_to_path(""), None);
        assert_eq!(file_url_to_path("file://"), None);
    }

    #[test]
    fn broken_escapes_stay_literal() {
        // A truncated or non-hex escape is kept as typed (as the C++ test expects), not decoded.
        assert_eq!(file_url_to_path("file:///a%2"), Some(native("/a%2")));
        assert_eq!(file_url_to_path("file:///a%zz"), Some(native("/a%zz")));
    }

    #[test]
    fn drive_letter_and_unc_hosts() {
        if !cfg!(windows) {
            assert_eq!(
                file_url_to_path("file://localhost/C:/Users/dj/a%20b.wav").as_deref(),
                Some("C:/Users/dj/a b.wav")
            );
            assert_eq!(
                file_url_to_path("file:///C:/Users/dj/a.wav").as_deref(),
                Some("C:/Users/dj/a.wav")
            );
            assert_eq!(
                file_url_to_path("file://server/share/a.wav").as_deref(),
                Some("//server/share/a.wav")
            );
        }
    }
}
