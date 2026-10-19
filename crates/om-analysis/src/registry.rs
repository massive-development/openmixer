//! Id -> analyser map shared by the service and plugins. Thread-safe; a running job keeps its
//! analyser alive (`Arc`) even if it is removed meanwhile. Port of `src/analysis/AnalyzerRegistry.*`.

use crate::analyzer::INTERFACE_VERSION;
use crate::{Analyzer, BpmAnalyzer, KeyAnalyzer, StructureAnalyzer, WaveformAnalyzer};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("analyzer interface version mismatch: {0}")]
    InterfaceMismatch(String),
    #[error("analyzer id is empty")]
    EmptyId,
    #[error("analyzer already registered: {0}")]
    Duplicate(String),
}

#[derive(Default)]
pub struct AnalyzerRegistry {
    analyzers: Mutex<BTreeMap<String, Arc<dyn Analyzer>>>,
}

impl AnalyzerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The built-in analysers: bpm (beat grid), key, waveform and structure. Port of
    /// `builtInAnalyzers()` in `src/ui/LibraryServices.cpp`.
    pub fn builtin() -> Self {
        let registry = Self::new();
        let builtins: [Arc<dyn Analyzer>; 4] = [
            Arc::new(BpmAnalyzer::default()),
            Arc::new(KeyAnalyzer::default()),
            Arc::new(WaveformAnalyzer::default()),
            Arc::new(StructureAnalyzer::default()),
        ];
        for analyzer in builtins {
            registry
                .add(analyzer)
                .expect("built-in analyser ids are unique");
        }
        registry
    }

    /// Registers `analyzer`. Rejects a wrong interface version, an empty id and a duplicate id.
    pub fn add(&self, analyzer: Arc<dyn Analyzer>) -> Result<(), RegistryError> {
        if analyzer.interface_version() != INTERFACE_VERSION {
            return Err(RegistryError::InterfaceMismatch(analyzer.id().to_owned()));
        }
        let id = analyzer.id().to_owned();
        if id.is_empty() {
            return Err(RegistryError::EmptyId);
        }
        let mut analyzers = self.lock();
        if analyzers.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        analyzers.insert(id, analyzer);
        Ok(())
    }

    /// True when an analyser with `id` was removed.
    pub fn remove(&self, id: &str) -> bool {
        self.lock().remove(id).is_some()
    }

    pub fn find(&self, id: &str) -> Option<Arc<dyn Analyzer>> {
        self.lock().get(id).cloned()
    }

    /// Registered ids in sorted order.
    pub fn ids(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Arc<dyn Analyzer>>> {
        self.analyzers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;
