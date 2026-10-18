// Import extension point: an importer recognises one foreign library format (by its XML root) and
// parses it into the neutral ImportedLibrary model. Plugins register further importers (Traktor,
// Serato, ...) without touching the writer or the facade.
#pragma once

#include "ImportModel.h"

#include <memory>
#include <vector>

namespace openmixer::library
{

class LibraryImporter
{
public:
    virtual ~LibraryImporter() = default;

    virtual juce::String name() const = 0;
    // Cheap check of the document root; both supported formats share the .xml extension.
    virtual bool recognises (const juce::XmlElement& root) const = 0;
    // Pure parsing, no database access. Unusable entries are counted in report, not thrown.
    virtual juce::Result parse (const juce::XmlElement& root, ImportedLibrary& out, ImportReport& report) const = 0;
};

// Later registrations win, so a better importer can override a default.
class LibraryImporterRegistry
{
public:
    void add (std::shared_ptr<LibraryImporter> importer);
    const LibraryImporter* find (const juce::XmlElement& root) const;
    juce::StringArray names() const;

    // Registry preloaded with the iTunes/Music and Rekordbox importers.
    static LibraryImporterRegistry withDefaults();

private:
    std::vector<std::shared_ptr<LibraryImporter>> importers;
};

} // namespace openmixer::library
