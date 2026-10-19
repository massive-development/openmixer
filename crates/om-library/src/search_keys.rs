//! Key-filter helper: resolves "8A" / "Am" (and compatible neighbours) to Camelot codes.
//! Port of `src/library/SearchKeys.{h,cpp}`.

use om_analysis::{is_harmonically_compatible, MusicalKey};

/// Empty when `text` is not a key. With `compatible`, includes the keys that mix harmonically (same
/// slot, +-1 on the wheel, relative), in the order the C++ enumerates them.
pub fn key_camelot_codes(text: &str, compatible: bool) -> Vec<String> {
    let mut codes = Vec::new();
    let Some(base) = resolve(text) else {
        return codes;
    };

    codes.push(base.camelot());
    if !compatible {
        return codes;
    }

    for tonic in 0..12 {
        for minor in [false, true] {
            let candidate = MusicalKey::new(tonic, minor);
            if is_harmonically_compatible(&base, &candidate) {
                let code = candidate.camelot();
                if !codes.contains(&code) {
                    codes.push(code);
                }
            }
        }
    }
    codes
}

fn resolve(text: &str) -> Option<MusicalKey> {
    let t = text.trim();
    // `from_camelot` upper-cases its input, as the C++ `toUpperCase` did.
    MusicalKey::from_camelot(t).or_else(|| MusicalKey::from_name(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_camelot_and_name_resolve_to_one_code() {
        assert_eq!(key_camelot_codes("8a", false), vec!["8A"]);
        assert_eq!(key_camelot_codes(" Am ", false), vec!["8A"]);
        assert!(key_camelot_codes("H#", false).is_empty());
    }

    #[test]
    fn compatible_keys_include_neighbours_and_relative() {
        let mut codes = key_camelot_codes("8A", true);
        codes.sort();
        assert_eq!(codes, vec!["7A", "8A", "8B", "9A"]);
    }
}
