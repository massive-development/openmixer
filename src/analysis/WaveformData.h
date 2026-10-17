// Multi-resolution waveform for overview and zoomed deck displays. Level 0 is the finest; each
// further level merges kLevelFactor points, down to roughly kOverviewPoints. Each point holds the
// full-band peak range and RMS of three bands (low < 250 Hz, mid, high > 4 kHz) for colouring.
// Immutable after building; safe to share between threads.
#pragma once

#include <juce_core/juce_core.h>

#include <optional>
#include <vector>

namespace openmixer::analysis
{

struct WaveformPoint
{
    float min = 0.0f, max = 0.0f;             // full-band sample range, [-1, 1]
    float low = 0.0f, mid = 0.0f, high = 0.0f; // band RMS, [0, 1]
};

struct WaveformLevel
{
    int samplesPerPoint = 0;
    std::vector<WaveformPoint> points;
};

class WaveformData
{
public:
    static constexpr int kFormatVersion = 1;
    static constexpr int kLevelFactor = 4;
    static constexpr int kOverviewPoints = 2048;

    WaveformData() = default;
    WaveformData (double sampleRate, juce::int64 numSamples, WaveformLevel base);

    bool isValid() const noexcept { return ! levels.empty() && ! levels.front().points.empty(); }
    double getSampleRate() const noexcept { return rate; }
    juce::int64 getNumSamples() const noexcept { return length; }
    const std::vector<WaveformLevel>& getLevels() const noexcept { return levels; }

    // Coarsest level that still has at least one point per pixel at this zoom.
    const WaveformLevel& levelFor (double samplesPerPixel) const noexcept;
    const WaveformLevel& getOverview() const noexcept { return levels.back(); }

    // Compact payload: the base level quantised to 8 bits per value, base64 in JSON.
    juce::String toJson() const;
    static std::optional<WaveformData> fromJson (const juce::String& json);

private:
    void buildCoarserLevels();

    double rate = 0.0;
    juce::int64 length = 0;
    std::vector<WaveformLevel> levels;
};

} // namespace openmixer::analysis
