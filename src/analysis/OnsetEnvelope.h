// Onset strength over time: positive log-spectral flux within frequency bands, then the local
// mean removed and half-wave rectified. Shared by tempo, downbeat and structure analysis.
#pragma once

#include "Analyzer.h"

#include <vector>

namespace openmixer::analysis
{

struct FrequencyBand
{
    double lowHz = 30.0;
    double highHz = 16000.0;
};

struct OnsetOptions
{
    int fftOrder = 10;   // 1024-sample frames
    int hop = 512;
    double meanWindowSeconds = 0.5;   // 0 keeps the raw flux
};

struct OnsetEnvelope
{
    std::vector<float> values;
    double frameRate = 0.0;     // values per second
    double startSeconds = 0.0;  // time of values[0]

    double timeOf (double index) const noexcept { return startSeconds + index / frameRate; }
    double indexOf (double seconds) const noexcept { return (seconds - startSeconds) * frameRate; }
    float at (double index) const noexcept;   // linear interpolation, 0 outside
};

// One envelope per band from a single STFT pass; nullopt when cancelled. Reports progress.
std::optional<std::vector<OnsetEnvelope>> computeBandOnsets (const AudioClip& clip, const OnsetOptions& options,
                                                             const std::vector<FrequencyBand>& bands,
                                                             AnalysisContext& context);

} // namespace openmixer::analysis
