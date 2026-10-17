// Stem separation extension point. A separator turns one clip into named stems (same length and
// rate as the input). Backends (the planned Demucs ONNX one, or plugins) implement this; calls
// may run on analysis worker threads, so implementations must be thread-safe or per-thread.
#pragma once

#include "analysis/Analyzer.h"

#include <map>

namespace openmixer::stems
{

enum class StemKind { Drums, Bass, Vocals, Other };

juce::String toString (StemKind kind);

struct StemSet
{
    double sampleRate = 0.0;
    std::map<StemKind, std::vector<float>> stems;   // mono samples per stem

    bool isComplete (size_t expectedLength) const noexcept;
};

class StemSeparator
{
public:
    static constexpr int kInterfaceVersion = 1;

    virtual ~StemSeparator() = default;

    virtual int getInterfaceVersion() const noexcept { return kInterfaceVersion; }
    virtual juce::String getId() const = 0;
    virtual int getVersion() const = 0;   // part of the cache key for stored stems

    // nullopt when cancelled or the model is unavailable.
    virtual std::optional<StemSet> separate (const analysis::AudioClip& clip, analysis::AnalysisContext& context) const = 0;
};

} // namespace openmixer::stems
