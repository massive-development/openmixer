#include "OnsetEnvelope.h"

#include "Stft.h"

#include <cmath>

namespace openmixer::analysis
{
namespace
{
constexpr float kLogGain = 1000.0f;

struct BinRange { size_t first, last; };

BinRange binsOf (const MagnitudeStft& stft, const FrequencyBand& band, double sampleRate)
{
    const auto binHz = stft.getBinHz (1, sampleRate);
    const auto top = stft.getNumBins() - 1;
    const auto first = juce::jlimit (1, top, static_cast<int> (std::ceil (band.lowHz / binHz)));
    const auto last = juce::jlimit (first, top, static_cast<int> (std::floor (band.highHz / binHz)));
    return { static_cast<size_t> (first), static_cast<size_t> (last) };
}

// Log magnitudes are computed once per frame and shared by all bands.
void logCompress (const std::vector<float>& magnitudes, std::vector<float>& levels)
{
    levels.resize (magnitudes.size());
    for (size_t k = 0; k < magnitudes.size(); ++k)
        levels[k] = std::log1p (kLogGain * magnitudes[k]);
}

float positiveFlux (const std::vector<float>& now, const std::vector<float>& previous, BinRange bins)
{
    float flux = 0.0f;
    for (auto k = bins.first; k <= bins.last; ++k)
        flux += std::max (0.0f, now[k] - previous[k]);
    return flux;
}

OnsetEnvelope emptyEnvelope (const MagnitudeStft& stft, double sampleRate, int frames)
{
    OnsetEnvelope env;
    env.frameRate = sampleRate / stft.getHop();
    env.startSeconds = 0.5 * stft.getFrameSize() / sampleRate;   // frame centre
    env.values.reserve (static_cast<size_t> (frames));
    return env;
}

void removeLocalMean (std::vector<float>& v, int radius)
{
    if (radius <= 0 || v.empty())
        return;
    std::vector<double> prefix (v.size() + 1, 0.0);
    for (size_t i = 0; i < v.size(); ++i)
        prefix[i + 1] = prefix[i] + v[i];

    const auto n = static_cast<long> (v.size());
    std::vector<float> out (v.size());
    for (long i = 0; i < n; ++i)
    {
        const auto lo = std::max (0L, i - radius), hi = std::min (n, i + radius + 1);
        const auto mean = (prefix[static_cast<size_t> (hi)] - prefix[static_cast<size_t> (lo)]) / static_cast<double> (hi - lo);
        out[static_cast<size_t> (i)] = std::max (0.0f, v[static_cast<size_t> (i)] - static_cast<float> (mean));
    }
    v.swap (out);
}
} // namespace

float OnsetEnvelope::at (double index) const noexcept
{
    if (index < 0.0 || values.empty())
        return 0.0f;
    const auto i = static_cast<size_t> (index);
    if (i + 1 >= values.size())
        return i < values.size() ? values[i] : 0.0f;
    const auto frac = static_cast<float> (index - static_cast<double> (i));
    return values[i] + frac * (values[i + 1] - values[i]);
}

std::optional<std::vector<OnsetEnvelope>> computeBandOnsets (const AudioClip& clip, const OnsetOptions& options,
                                                             const std::vector<FrequencyBand>& bands,
                                                             AnalysisContext& context)
{
    MagnitudeStft stft (options.fftOrder, options.hop);
    const auto frames = stft.getNumFrames (clip.samples.size());
    std::vector<BinRange> ranges;
    std::vector<OnsetEnvelope> envelopes;
    for (const auto& band : bands)
    {
        ranges.push_back (binsOf (stft, band, clip.sampleRate));
        envelopes.push_back (emptyEnvelope (stft, clip.sampleRate, frames));
    }

    std::vector<float> magnitudes, levels, previous (static_cast<size_t> (stft.getNumBins()), 0.0f);
    for (int i = 0; i < frames; ++i)
    {
        if ((i & 255) == 0 && context.isCancelled())
            return std::nullopt;
        stft.computeFrame (clip.samples, i, magnitudes);
        logCompress (magnitudes, levels);
        for (size_t b = 0; b < ranges.size(); ++b)
            envelopes[b].values.push_back (positiveFlux (levels, previous, ranges[b]));
        previous.swap (levels);
        context.setProgress (static_cast<float> (i + 1) / static_cast<float> (frames));
    }
    for (auto& env : envelopes)
        removeLocalMean (env.values, static_cast<int> (options.meanWindowSeconds * env.frameRate * 0.5));
    return envelopes;
}

} // namespace openmixer::analysis
