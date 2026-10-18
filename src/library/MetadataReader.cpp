#include "MetadataReader.h"

#include "JuceMetadataReader.h"

namespace openmixer::library
{

void MetadataReaderRegistry::add (std::shared_ptr<MetadataReader> reader)
{
    if (reader != nullptr)
        readers.push_back (std::move (reader));
}

MetadataReader* MetadataReaderRegistry::find (const juce::File& file) const
{
    const auto ext = file.getFileExtension().trimCharactersAtStart (".").toLowerCase();
    for (auto it = readers.rbegin(); it != readers.rend(); ++it)   // newest first
        if ((*it)->extensions().contains (ext))
            return it->get();
    return nullptr;
}

juce::StringArray MetadataReaderRegistry::supportedExtensions() const
{
    juce::StringArray all;
    for (const auto& r : readers)
        all.addArray (r->extensions());
    all.removeDuplicates (false);
    return all;
}

MetadataReaderRegistry MetadataReaderRegistry::withDefaults()
{
    MetadataReaderRegistry registry;
    registry.add (std::make_shared<JuceMetadataReader>());
    return registry;
}

} // namespace openmixer::library
