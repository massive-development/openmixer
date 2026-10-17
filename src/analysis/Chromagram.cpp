#include "Chromagram.h"

#include "Stft.h"

#include <algorithm>
#include <cmath>

namespace openmixer::analysis
{
namespace
{
constexpr double kSilenceFloor = 0.01;   // on the sqrt-magnitude sum: about -80 dB

struct BinWeight { size_t bin; size_t pitchClass; float weight; };

// Bins near a semitone centre count fully, fading to zero half way between semitones.
std::vector<BinWeight> buildBinMap (const MagnitudeStft& stft, const ChromaOptions& o, double sampleRate)
{
    std::vector<BinWeight> map;
    for (int k = 1; k < stft.getNumBins(); ++k)
    {
        const auto hz = stft.getBinHz (k, sampleRate);
        if (hz < o.minHz || hz > o.maxHz)
            continue;
        const auto midi = 69.0 + 12.0 * std::log2 (hz / o.tuningHz);
        const auto nearest = std::round (midi);
        const auto weight = 1.0 - 2.0 * std::abs (midi - nearest);
        if (weight > 0.0)
            map.push_back ({ static_cast<size_t> (k), static_cast<size_t> (((static_cast<long> (nearest) % 12) + 12) % 12),
                             static_cast<float> (weight) });
    }
    return map;
}

Chroma frameChroma (const std::vector<float>& magnitudes, const std::vector<BinWeight>& map)
{
    Chroma c {};
    for (const auto& w : map)
        c[w.pitchClass] += w.weight * std::sqrt (magnitudes[w.bin]);
    return c;
}

void accumulateNormalised (Chroma& total, const Chroma& frame)
{
    const auto peak = *std::max_element (frame.begin(), frame.end());
    if (peak < kSilenceFloor)
        return;
    for (size_t i = 0; i < 12; ++i)
        total[i] += frame[i] / peak;
}
} // namespace

std::optional<Chroma> computeChroma (const AudioClip& clip, const ChromaOptions& options, AnalysisContext& context)
{
    MagnitudeStft stft (options.fftOrder, options.hop);
    const auto map = buildBinMap (stft, options, clip.sampleRate);
    const auto frames = stft.getNumFrames (clip.samples.size());

    Chroma total {};
    std::vector<float> magnitudes;
    for (int i = 0; i < frames; ++i)
    {
        if ((i & 63) == 0 && context.isCancelled())
            return std::nullopt;
        stft.computeFrame (clip.samples, i, magnitudes);
        accumulateNormalised (total, frameChroma (magnitudes, map));
        context.setProgress (static_cast<float> (i + 1) / static_cast<float> (frames));
    }
    return total;
}

} // namespace openmixer::analysis
