//! One-call import: parse a foreign library XML with a registered importer and merge it into the
//! `Library` in one transaction. Port of `src/library/LibraryImport.*`.

use crate::error::{LibraryError, Result};
use crate::import_model::{ImportReport, ImportedLibrary};
use crate::import_writer::ImportWriter;
use crate::library::Library;
use crate::library_importer::LibraryImporterRegistry;
use crate::xml::read_xml_file;
use std::path::Path;

/// Fails (library untouched) when the file is unreadable, not recognised, or a database write
/// fails.
pub fn import_library_file(
    library: &Library,
    importers: &LibraryImporterRegistry,
    xml_file: &Path,
    report: &mut ImportReport,
) -> Result<()> {
    let root = read_xml_file(xml_file).map_err(|e| {
        LibraryError::Import(format!("Cannot read {}: {e}", xml_file.display()))
    })?;
    let importer = importers.find(&root).ok_or_else(|| {
        LibraryError::Import(format!(
            "Unrecognised library format: {}",
            xml_file.file_name().unwrap_or_default().to_string_lossy()
        ))
    })?;
    let mut data = ImportedLibrary::default();
    importer
        .parse(&root, &mut data, report)
        .map_err(LibraryError::Import)?;
    ImportWriter::new(library, report).write(&data)
}
