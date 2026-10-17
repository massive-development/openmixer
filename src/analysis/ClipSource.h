// Where a job's audio comes from. The content hash keys the cache, so it must depend only on the
// audio bytes (renaming or moving a file keeps its results). Called on a worker thread.
#pragma once

#include "Analyzer.h"

#include <juce_audio_formats/juce_audio_formats.h>

namespace openmixer::analysis
{

class ClipSource
{
public:
    virtual ~ClipSource() = default;

    virtual juce::String getContentHash() = 0;   // empty when the source is unreadable
    virtual std::optional<AudioClip> load (AnalysisContext& context) = 0;
    virtual juce::String describe() const = 0;
};

// Any format JUCE can decode; channels are averaged to mono at the file's own rate.
class FileClipSource final : public ClipSource
{
public:
    explicit FileClipSource (juce::File fileToRead) : file (std::move (fileToRead)) {}

    juce::String getContentHash() override;
    std::optional<AudioClip> load (AnalysisContext& context) override;
    juce::String describe() const override { return file.getFullPathName(); }

private:
    juce::File file;
};

// Already-decoded audio (tests, stems, recordings); hashed from the samples and rate.
class MemoryClipSource final : public ClipSource
{
public:
    MemoryClipSource (AudioClip clipToUse, juce::String name) : clip (std::move (clipToUse)), label (std::move (name)) {}

    juce::String getContentHash() override;
    std::optional<AudioClip> load (AnalysisContext&) override { return clip; }
    juce::String describe() const override { return label; }

private:
    AudioClip clip;
    juce::String label;
};

juce::String hashFileContent (const juce::File& file);
juce::String hashClip (const AudioClip& clip);

} // namespace openmixer::analysis
