// Builds WaveformData from a clip: splits it into three bands with biquad crossovers, then
// reduces blocks of samplesPerPoint samples to one point. Stateless and thread-safe.
// WaveformAnalyzer wraps it for AnalysisService (id "waveform").
#pragma once

#include "Analyzer.h"
#include "WaveformData.h"

namespace openmixer::analysis
{

struct WaveformOptions
{
    int samplesPerPoint = 128;   // finest level: ~2.9 ms at 44.1 kHz
    float lowCrossoverHz = 250.0f;
    float highCrossoverHz = 4000.0f;
};

class WaveformBuilder
{
public:
    explicit WaveformBuilder (WaveformOptions opts = {}) : options (opts) {}

    // nullopt when cancelled; an invalid (empty) WaveformData for an empty clip.
    std::optional<WaveformData> build (const AudioClip& clip, AnalysisContext& context) const;

private:
    WaveformOptions options;
};

class WaveformAnalyzer final : public Analyzer
{
public:
    juce::String getId() const override { return "waveform"; }
    int getVersion() const override { return 1; }
    std::optional<juce::String> analyze (const AudioClip& clip, AnalysisContext& context) const override;

private:
    WaveformBuilder builder;
};

} // namespace openmixer::analysis
