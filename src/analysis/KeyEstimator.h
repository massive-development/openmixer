// Global key estimation: clip chroma correlated against 24 rotated key templates (12 major,
// 12 minor). Stateless and thread-safe. KeyAnalyzer wraps it for AnalysisService (id "key").
#pragma once

#include "Chromagram.h"
#include "MusicalKey.h"

namespace openmixer::analysis
{

struct KeyResult
{
    MusicalKey key;
    double confidence = 0.0;   // 0..1: margin of the best template over the runner-up
    Chroma chroma {};          // normalised to a peak of 1
    bool tonal = true;         // false: silent/atonal, key is meaningless

    juce::String toJson() const;
    static std::optional<KeyResult> fromJson (const juce::String& json);
};

class KeyEstimator
{
public:
    explicit KeyEstimator (ChromaOptions opts = {}) : options (opts) {}

    // nullopt when cancelled or without tonal content.
    std::optional<KeyResult> estimate (const AudioClip& clip, AnalysisContext& context) const;

    // Best key for a chroma vector (exposed for tests and other chroma sources such as stems).
    static std::optional<KeyResult> classify (const Chroma& chroma);

private:
    ChromaOptions options;
};

class KeyAnalyzer final : public Analyzer
{
public:
    juce::String getId() const override { return "key"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext& context) const override;

private:
    KeyEstimator estimator;
};

} // namespace openmixer::analysis
