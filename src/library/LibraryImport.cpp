#include "LibraryImport.h"

#include "ImportWriter.h"

namespace openmixer::library
{

juce::Result importLibraryFile (Library& library, const LibraryImporterRegistry& importers,
                                const juce::File& xmlFile, ImportReport& report)
{
    juce::XmlDocument doc (xmlFile);
    const auto root = doc.getDocumentElement();
    if (root == nullptr)
        return juce::Result::fail ("Cannot read " + xmlFile.getFullPathName() + ": " + doc.getLastParseError());
    const auto* importer = importers.find (*root);
    if (importer == nullptr)
        return juce::Result::fail ("Unrecognised library format: " + xmlFile.getFileName());
    ImportedLibrary data;
    if (auto parsed = importer->parse (*root, data, report); parsed.failed())
        return parsed;
    return ImportWriter (library, report).write (data);
}

} // namespace openmixer::library
