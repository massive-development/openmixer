// Constant-tempo BPM + beat grid estimation: full-band onset envelope -> autocorrelation tempo ->
// joint tempo/phase refinement -> downbeat from the low (kick) band. Stateless and thread-safe.
// BpmAnalyzer wraps it for AnalysisService (id "bpm").
#pragma once

#include "Analyzer.h"
#include "BeatGridModel.h"

namespace openmixer::analysis
{

struct BpmOptions
{
    double minBpm = 70.0;    // result is folded by octaves into [minBpm, maxBpm)
    double maxBpm = 180.0;
    int beatsPerBar = 4;
};

struct BpmResult
{
    double bpm = 0.0;
    double confidence = 0.0;   // 0..1, how strongly onsets line up with the grid
    BeatGridModel grid;

    juce::String toJson() const;
    static std::optional<BpmResult> fromJson (const juce::String& json);
};

class BpmEstimator
{
public:
    explicit BpmEstimator (BpmOptions opts = {}) : options (opts) {}

    // nullopt when cancelled, too short (< ~4 s) or without a detectable pulse.
    std::optional<BpmResult> estimate (const AudioClip& clip, AnalysisContext& context) const;

private:
    BpmOptions options;
};

class BpmAnalyzer final : public Analyzer
{
public:
    explicit BpmAnalyzer (BpmOptions opts = {}) : estimator (opts) {}

    juce::String getId() const override { return "bpm"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext& context) const override;

private:
    BpmEstimator estimator;
};

} // namespace openmixer::analysis
