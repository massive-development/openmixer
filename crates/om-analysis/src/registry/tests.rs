//! The `AnalyzerRegistry` case.

use super::*;
use crate::{AnalysisContext, AudioClip};
use std::sync::Arc;

struct NamedAnalyzer {
    name: &'static str,
    interface: i32,
}

impl Analyzer for NamedAnalyzer {
    fn interface_version(&self) -> i32 {
        self.interface
    }

    fn id(&self) -> &str {
        self.name
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, _clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        Some("{}".to_owned())
    }
}

fn named(name: &'static str) -> Arc<NamedAnalyzer> {
    Arc::new(NamedAnalyzer {
        name,
        interface: INTERFACE_VERSION,
    })
}

#[test]
fn rejects_duplicates_empty_ids_and_wrong_interface() {
    let registry = AnalyzerRegistry::new();
    assert!(registry.add(named("bpm")).is_ok());
    assert_eq!(
        registry.add(named("bpm")),
        Err(RegistryError::Duplicate("bpm".into()))
    );
    assert_eq!(registry.add(named("")), Err(RegistryError::EmptyId));
    let old = Arc::new(NamedAnalyzer {
        name: "v0",
        interface: 0,
    });
    assert!(matches!(
        registry.add(old),
        Err(RegistryError::InterfaceMismatch(_))
    ));

    assert_eq!(registry.ids(), vec!["bpm".to_owned()]);
    assert!(registry.find("bpm").is_some());
    assert!(registry.remove("bpm"));
    assert!(registry.find("bpm").is_none());
}

#[test]
fn builtin_registry_holds_the_four_analysers() {
    let registry = AnalyzerRegistry::builtin();
    assert_eq!(registry.ids(), vec!["bpm", "key", "structure", "waveform"]);
}
