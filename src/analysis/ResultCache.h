// Analysis result store keyed by (content hash, analyser id); a version mismatch counts as a miss.
// Implementations must be safe to call from several worker threads.
#pragma once

#include <juce_core/juce_core.h>

#include <optional>

namespace openmixer::analysis
{

class ResultCache
{
public:
    virtual ~ResultCache() = default;

    virtual std::optional<juce::String> find (const juce::String& contentHash, const juce::String& analyzerId,
                                              int version) = 0;
    virtual bool store (const juce::String& contentHash, const juce::String& analyzerId, int version,
                        const juce::String& payload) = 0;
    virtual int erase (const juce::String& contentHash) = 0;   // number of rows removed
};

} // namespace openmixer::analysis
