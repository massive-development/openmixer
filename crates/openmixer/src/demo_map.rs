//! `--demo-map`: the three test controller maps under `fixtures/controllers` merged into one map,
//! so the mapping editor opens populated. The merged map is written to a temporary file (never the
//! user's map). Nothing here touches MIDI hardware.

use std::path::{Path, PathBuf};

use om_midi::{MidiError, MidiMap};

/// The fixture maps merged into the demo, embedded at build time (no working-directory lookup).
const FIXTURES: [(&str, &str); 3] = [
    (
        "test_map.json",
        include_str!("../../../fixtures/controllers/test_map.json"),
    ),
    (
        "stem_map.json",
        include_str!("../../../fixtures/controllers/stem_map.json"),
    ),
    (
        "sampler_map.json",
        include_str!("../../../fixtures/controllers/sampler_map.json"),
    ),
];

/// Name of the temporary map file the demo writes.
pub const DEMO_MAP_FILE: &str = "demo-map.json";

/// The fixture maps parsed and merged: bindings and feedback rules concatenated in fixture order.
pub fn merged_demo_map() -> Result<MidiMap, MidiError> {
    let mut merged = MidiMap {
        name: "Demo controllers".to_owned(),
        ..MidiMap::default()
    };
    for (_, text) in FIXTURES {
        let map = MidiMap::parse(text)?;
        merged.mappings.extend(map.mappings);
        merged.outputs.extend(map.outputs);
    }
    Ok(merged)
}

/// Writes the merged demo map into `dir` (created when missing) and returns the file's path.
pub fn write_demo_map(dir: &Path) -> Result<PathBuf, String> {
    let map = merged_demo_map().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(DEMO_MAP_FILE);
    std::fs::write(&path, map.to_json()).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_map_holds_every_fixture_binding() {
        let merged = merged_demo_map().expect("fixtures parse");
        let expected: usize = FIXTURES
            .iter()
            .map(|(_, text)| MidiMap::parse(text).expect("fixture parses").mappings.len())
            .sum();
        assert_eq!(merged.mappings.len(), expected);
        assert!(expected > 10, "the demo needs a populated editor");
    }

    #[test]
    fn written_demo_map_loads_back_into_a_map() {
        let dir = std::env::temp_dir().join(format!("om-demo-map-test-{}", std::process::id()));
        let path = write_demo_map(&dir).expect("write");
        let text = std::fs::read_to_string(&path).expect("read back");
        let loaded = MidiMap::parse(&text).expect("written map parses");
        assert_eq!(
            loaded.mappings.len(),
            merged_demo_map().unwrap().mappings.len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
