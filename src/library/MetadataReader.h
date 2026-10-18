// Tag-reading extension point: readers register the file extensions they handle, the ingestor asks
// the registry for one. Plugins add readers (e.g. a TagLib-backed one) without touching the ingestor.
#pragma once

#include <juce_core/juce_core.h>

#include <memory>
#include <vector>

namespace openmixer::library
{

// What a reader can tell about a file; empty/zero means "not present in the file".
struct TrackMetadata
{
    juce::String title, artist, album, genre, comment;
    int year = 0;
    double durationSec = 0.0;
};

class MetadataReader
{
public:
    virtual ~MetadataReader() = default;

    virtual juce::String name() const = 0;
    // Lower-case extensions without the dot, e.g. "wav".
    virtual juce::StringArray extensions() const = 0;
    // Fails when the file cannot be decoded; called from the ingest thread only.
    virtual juce::Result read (const juce::File& file, TrackMetadata& out) = 0;
};

// Later registrations win for an extension, so a better reader can override a default.
class MetadataReaderRegistry
{
public:
    void add (std::shared_ptr<MetadataReader> reader);
    MetadataReader* find (const juce::File& file) const;
    bool supports (const juce::File& file) const { return find (file) != nullptr; }
    juce::StringArray supportedExtensions() const;

    // Registry preloaded with the built-in JUCE reader.
    static MetadataReaderRegistry withDefaults();

private:
    std::vector<std::shared_ptr<MetadataReader>> readers;
};

} // namespace openmixer::library
