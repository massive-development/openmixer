// Pitch-class profile of a clip: long-window STFT bins folded onto 12 semitone classes (A4 = 440 Hz),
// each frame normalised so loud passages do not dominate, then averaged over the clip.
#pragma once

#include "Analyzer.h"

#include <array>

namespace openmixer::analysis
{

using Chroma = std::array<double, 12>;   // index 0 = C

struct ChromaOptions
{
    int fftOrder = 13;   // 8192-sample frames: ~5 Hz bins at 44.1 kHz
    int hop = 4096;
    double minHz = 100.0;
    double maxHz = 5000.0;
    double tuningHz = 440.0;
};

// nullopt when cancelled; all zeros for silence.
std::optional<Chroma> computeChroma (const AudioClip& clip, const ChromaOptions& options, AnalysisContext& context);

} // namespace openmixer::analysis
