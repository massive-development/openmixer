#include "Mixer.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr float kMeterDecaySeconds = 0.3F;

    float clampGain (float g) noexcept { return std::clamp (g, 0.0F, Mixer::kMaxGain); }
}

Mixer::Mixer (int n) : numChannels (std::clamp (n, 1, kMaxChannels)) {}

ChannelStrip& Mixer::strip (int ch) noexcept
{
    return channels[static_cast<size_t> (std::clamp (ch, 0, numChannels - 1))];
}

const ChannelStrip& Mixer::strip (int ch) const noexcept
{
    return channels[static_cast<size_t> (std::clamp (ch, 0, numChannels - 1))];
}

void Mixer::prepare (double sr, int maxBlockSize)
{
    sampleRate = sr > 0.0 ? sr : 44100.0;
    maxBlock = std::max (1, maxBlockSize);
    for (auto& c : channels)
        c.prepare (sampleRate, maxBlock);
    masterInserts.prepare (sampleRate, maxBlock);
    masterPeakL.reset();
    masterPeakR.reset();
}

void Mixer::setMasterGain (float g) noexcept { masterGain.store (clampGain (g), kRelaxed); }
void Mixer::setCueLevel (float g) noexcept   { cueLevel.store (clampGain (g), kRelaxed); }
void Mixer::setCueMix (float m) noexcept     { cueMix.store (std::clamp (m, 0.0F, 1.0F), kRelaxed); }

void Mixer::process (const ChannelInput* inputs, float* mL, float* mR, float* cL, float* cR, int numSamples) noexcept
{
    for (int done = 0; done < numSamples; done += maxBlock)
    {
        const int n = std::min (maxBlock, numSamples - done);
        ChannelInput shifted[kMaxChannels];
        for (int c = 0; c < numChannels; ++c)
        {
            shifted[c].left  = inputs[c].left  != nullptr ? inputs[c].left  + done : nullptr;
            shifted[c].right = inputs[c].right != nullptr ? inputs[c].right + done : nullptr;
        }
        processChunk (shifted, { mL + done, mR + done, cL + done, cR + done }, n);
    }
}

void Mixer::processChunk (const ChannelInput* inputs, const MixBus& bus, int n) noexcept
{
    std::fill (bus.masterL, bus.masterL + n, 0.0F);
    std::fill (bus.masterR, bus.masterR + n, 0.0F);
    std::fill (bus.cueL, bus.cueL + n, 0.0F);
    std::fill (bus.cueR, bus.cueR + n, 0.0F);

    const float decay = std::exp (-static_cast<float> (n) / (static_cast<float> (sampleRate) * kMeterDecaySeconds));
    const auto xf = crossfader.gains();
    for (int ch = 0; ch < numChannels; ++ch)
        channels[static_cast<size_t> (ch)].render (inputs[ch].left, inputs[ch].right, xf, bus, n, decay);

    finishMaster (bus, n, decay);
    mixHeadphones (bus, n);
}

void Mixer::finishMaster (const MixBus& bus, int n, float decay) noexcept
{
    const float gain = masterGain.load (kRelaxed);
    for (int i = 0; i < n; ++i)
    {
        bus.masterL[i] *= gain;
        bus.masterR[i] *= gain;
    }
    masterInserts.process (bus.masterL, bus.masterR, n);

    float peakL = 0.0F, peakR = 0.0F;
    for (int i = 0; i < n; ++i)
    {
        peakL = std::max (peakL, std::abs (bus.masterL[i]));
        peakR = std::max (peakR, std::abs (bus.masterR[i]));
    }
    masterPeakL.push (peakL, decay);
    masterPeakR.push (peakR, decay);
}

void Mixer::mixHeadphones (const MixBus& bus, int n) noexcept
{
    const float level = cueLevel.load (kRelaxed), mix = cueMix.load (kRelaxed);
    for (int i = 0; i < n; ++i)
    {
        bus.cueL[i] = level * ((1.0F - mix) * bus.cueL[i] + mix * bus.masterL[i]);
        bus.cueR[i] = level * ((1.0F - mix) * bus.cueR[i] + mix * bus.masterR[i]);
    }
}

} // namespace openmixer
