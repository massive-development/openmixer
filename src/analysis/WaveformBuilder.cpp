#include "WaveformBuilder.h"

#include "dsp/Biquad.h"

#include <array>
#include <cmath>

namespace openmixer::analysis
{
namespace
{
constexpr float kButterworthQ = 0.7071f;

// Two cascaded biquads per edge (24 dB/oct) keep the bands visually distinct.
class Cascade
{
public:
    void design (const dsp::BiquadCoeffs& c) noexcept { for (auto& s : stages) s.setCoeffs (c); }
    void process (float* samples, int n) noexcept { for (auto& s : stages) s.process (0, samples, n); }

private:
    std::array<dsp::Biquad, 2> stages;
};

class BandSplitter
{
public:
    BandSplitter (float rate, float lowHz, float highHz)
    {
        lowPass.design (dsp::designLowPass (rate, lowHz, kButterworthQ));
        midHigh.design (dsp::designHighPass (rate, lowHz, kButterworthQ));
        midLow.design (dsp::designLowPass (rate, highHz, kButterworthQ));
        highPass.design (dsp::designHighPass (rate, highHz, kButterworthQ));
    }

    void process (const float* in, int n, std::array<std::vector<float>, 3>& bands)
    {
        for (auto& b : bands)
            b.assign (in, in + n);
        lowPass.process (bands[0].data(), n);
        midHigh.process (bands[1].data(), n);
        midLow.process (bands[1].data(), n);
        highPass.process (bands[2].data(), n);
    }

private:
    Cascade lowPass, midHigh, midLow, highPass;
};

float rms (const std::vector<float>& v)
{
    double sum = 0.0;
    for (auto x : v)
        sum += static_cast<double> (x) * x;
    return v.empty() ? 0.0f : static_cast<float> (std::sqrt (sum / static_cast<double> (v.size())));
}

WaveformPoint reduceBlock (const float* in, int n, const std::array<std::vector<float>, 3>& bands)
{
    WaveformPoint p { 1.0f, -1.0f, rms (bands[0]), rms (bands[1]), rms (bands[2]) };
    for (int i = 0; i < n; ++i)
    {
        p.min = std::min (p.min, in[i]);
        p.max = std::max (p.max, in[i]);
    }
    return p;
}
} // namespace

std::optional<WaveformData> WaveformBuilder::build (const AudioClip& clip, AnalysisContext& context) const
{
    const auto total = clip.samples.size();
    const auto block = static_cast<size_t> (std::max (1, options.samplesPerPoint));
    BandSplitter splitter (static_cast<float> (clip.sampleRate), options.lowCrossoverHz, options.highCrossoverHz);
    std::array<std::vector<float>, 3> bands;
    WaveformLevel base { static_cast<int> (block), {} };
    base.points.reserve (total / block + 1);

    for (size_t start = 0; start < total; start += block)
    {
        if ((base.points.size() & 1023) == 0 && context.isCancelled())
            return std::nullopt;
        const auto n = static_cast<int> (std::min (block, total - start));
        splitter.process (clip.samples.data() + start, n, bands);
        base.points.push_back (reduceBlock (clip.samples.data() + start, n, bands));
        context.setProgress (static_cast<float> (start + static_cast<size_t> (n)) / static_cast<float> (total));
    }
    return WaveformData (clip.sampleRate, static_cast<juce::int64> (total), std::move (base));
}

std::optional<juce::String> WaveformAnalyzer::analyze (const AudioClip& clip, AnalysisContext& context) const
{
    const auto data = builder.build (clip, context);
    if (! data || context.isCancelled())
        return std::nullopt;
    return data->toJson();
}

} // namespace openmixer::analysis
