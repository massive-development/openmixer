// Default MetadataReader: JUCE's AudioFormatReader. Gives duration for every format JUCE decodes and
// RIFF INFO / format comments where the container exposes them (WAV); title falls back to the file name.
#pragma once

#include "MetadataReader.h"

#include <juce_audio_formats/juce_audio_formats.h>

namespace openmixer::library
{

class JuceMetadataReader final : public MetadataReader
{
public:
    JuceMetadataReader();

    juce::String name() const override { return "juce"; }
    juce::StringArray extensions() const override { return exts; }
    juce::Result read (const juce::File& file, TrackMetadata& out) override;

private:
    juce::AudioFormatManager formats;
    juce::StringArray exts;
};

} // namespace openmixer::library
