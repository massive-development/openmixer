//! Import extension point: an importer recognises one foreign library format (by its XML root)
//! and parses it into the neutral [`ImportedLibrary`] model. Later registrations win, so a better
//! importer can override a default.

use crate::error::ImportError;
use crate::import_model::{ImportReport, ImportedLibrary};
use crate::itunes_importer::ItunesImporter;
use crate::rekordbox_importer::RekordboxImporter;
use crate::xml::XmlElement;
use std::sync::Arc;

pub trait LibraryImporter: Send + Sync {
    fn name(&self) -> &str;

    /// Cheap check of the document root; both supported formats share the `.xml` extension.
    fn recognises(&self, root: &XmlElement) -> bool;

    /// Pure parsing, no database access. Unusable entries are counted in `report`, not thrown.
    /// An `Err` means the document cannot be parsed at all.
    fn parse(
        &self,
        root: &XmlElement,
        out: &mut ImportedLibrary,
        report: &mut ImportReport,
    ) -> Result<(), ImportError>;
}

#[derive(Default)]
pub struct LibraryImporterRegistry {
    importers: Vec<Arc<dyn LibraryImporter>>,
}

impl LibraryImporterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry preloaded with the iTunes/Music and Rekordbox importers.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.add(Arc::new(ItunesImporter));
        registry.add(Arc::new(RekordboxImporter));
        registry
    }

    pub fn add(&mut self, importer: Arc<dyn LibraryImporter>) {
        self.importers.push(importer);
    }

    /// The newest importer that recognises `root`.
    pub fn find(&self, root: &XmlElement) -> Option<&dyn LibraryImporter> {
        self.importers
            .iter()
            .rev()
            .find(|i| i.recognises(root))
            .map(|i| i.as_ref())
    }

    pub fn names(&self) -> Vec<String> {
        self.importers.iter().map(|i| i.name().to_owned()).collect()
    }
}
