// Track structure for Automix and cue suggestions: onset times, a loudness curve, labelled
// sections and suggested mix-in/mix-out points (start/end of the loud part). Stateless and
// thread-safe; StructureAnalyzer is the AnalysisService wrapper (id "structure").
#pragma once

#include "Analyzer.h"
#include "Segmenter.h"

namespace openmixer::analysis
{

struct StructureResult
{
    std::vector<double> onsetSeconds;
    EnergyCurve energy;
    std::vector<Section> sections;
    double mixInSeconds = 0.0;
    double mixOutSeconds = 0.0;

    juce::String toJson() const;
    static std::optional<StructureResult> fromJson (const juce::String& json);
};

class StructureEstimator
{
public:
    explicit StructureEstimator (SegmentOptions opts = {}) : options (opts) {}

    // nullopt when cancelled.
    std::optional<StructureResult> estimate (const AudioClip& clip, AnalysisContext& context) const;

private:
    SegmentOptions options;
};

class StructureAnalyzer final : public Analyzer
{
public:
    juce::String getId() const override { return "structure"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext& context) const override;

private:
    StructureEstimator estimator;
};

} // namespace openmixer::analysis
