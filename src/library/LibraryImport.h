// One-call import: parse a foreign library XML with a registered importer and merge it into the
// Library in one transaction. Report counts: added/merged/skipped tracks, cues, playlists, errors.
// Threading: runs on the thread that owns the Library.
#pragma once

#include "Library.h"
#include "LibraryImporter.h"

namespace openmixer::library
{

// Fails (library untouched) when the file is unreadable, not recognised, or a database write fails.
juce::Result importLibraryFile (Library& library, const LibraryImporterRegistry& importers,
                                const juce::File& xmlFile, ImportReport& report);

} // namespace openmixer::library
