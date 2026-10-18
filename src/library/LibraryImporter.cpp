#include "LibraryImporter.h"

#include "ItunesImporter.h"
#include "RekordboxImporter.h"

namespace openmixer::library
{

void ImportReport::note (const juce::String& message)
{
    constexpr int kMaxMessages = 20;   // a bad export could otherwise flood the report
    if (messages.size() < kMaxMessages)
        messages.add (message);
}

void LibraryImporterRegistry::add (std::shared_ptr<LibraryImporter> importer)
{
    if (importer != nullptr)
        importers.push_back (std::move (importer));
}

const LibraryImporter* LibraryImporterRegistry::find (const juce::XmlElement& root) const
{
    for (auto it = importers.rbegin(); it != importers.rend(); ++it)   // newest first
        if ((*it)->recognises (root))
            return it->get();
    return nullptr;
}

juce::StringArray LibraryImporterRegistry::names() const
{
    juce::StringArray all;
    for (const auto& i : importers)
        all.add (i->name());
    return all;
}

LibraryImporterRegistry LibraryImporterRegistry::withDefaults()
{
    LibraryImporterRegistry registry;
    registry.add (std::make_shared<ItunesImporter>());
    registry.add (std::make_shared<RekordboxImporter>());
    return registry;
}

} // namespace openmixer::library
