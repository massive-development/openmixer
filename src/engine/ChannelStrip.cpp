#include "ChannelStrip.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace { constexpr auto kRelaxed = std::memory_order_relaxed; }

void ChannelStrip::prepare (double sr, int maxBlockSize)
{
    scratchL.assign (static_cast<size_t> (maxBlockSize), 0.0F);
    scratchR.assign (static_cast<size_t> (maxBlockSize), 0.0F);
    eq.prepare (sr, maxBlockSize);
    filter.prepare (sr, maxBlockSize);
    inserts.prepare (sr, maxBlockSize);
    trimGain = trim.load (kRelaxed);
    faderGain = -1.0F;
    meter.reset();
}

void ChannelStrip::setTrim (float linear) noexcept { trim.store (std::clamp (linear, 0.0F, 4.0F), kRelaxed); }
void ChannelStrip::setFader (float v) noexcept { fader.store (std::clamp (v, 0.0F, 1.0F), kRelaxed); }

void ChannelStrip::applyTrim (const float* srcL, const float* srcR, int n) noexcept
{
    const float target = trim.load (kRelaxed);
    const float step = (target - trimGain) / static_cast<float> (n);
    float g = trimGain;
    for (int i = 0; i < n; ++i, g += step)
    {
        scratchL[static_cast<size_t> (i)] = srcL[i] * g;
        scratchR[static_cast<size_t> (i)] = srcR[i] * g;
    }
    trimGain = target;
}

float ChannelStrip::mixIntoMaster (float* const* buf, float target, const MixBus& bus, int n) noexcept
{
    const float step = (target - faderGain) / static_cast<float> (n);
    float g = faderGain, peak = 0.0F;
    for (int i = 0; i < n; ++i, g += step)
    {
        const float l = buf[0][i] * g, r = buf[1][i] * g;
        bus.masterL[i] += l;
        bus.masterR[i] += r;
        peak = std::max (peak, std::max (std::abs (l), std::abs (r)));
    }
    faderGain = target;
    return peak;
}

void ChannelStrip::render (const float* srcL, const float* srcR, const CrossfaderGains& xf,
                           const MixBus& bus, int n, float meterDecay) noexcept
{
    const float target = fader.load (kRelaxed) * Crossfader::gainFor (xf, getAssign());
    if (faderGain < 0.0F)
        faderGain = target;

    if (srcL == nullptr && srcR == nullptr)
    {
        faderGain = target;
        trimGain = trim.load (kRelaxed);
        meter.push (0.0F, meterDecay);
        return;
    }
    if (srcL == nullptr) srcL = srcR;
    if (srcR == nullptr) srcR = srcL;

    applyTrim (srcL, srcR, n);
    float* buf[2] { scratchL.data(), scratchR.data() };
    eq.process (buf[0], buf[1], n);
    filter.process (buf[0], buf[1], n);
    inserts.process (buf[0], buf[1], n);

    if (getPfl())
        for (int i = 0; i < n; ++i)
        {
            bus.cueL[i] += buf[0][i];
            bus.cueR[i] += buf[1][i];
        }
    meter.push (mixIntoMaster (buf, target, bus, n), meterDecay);
}

} // namespace openmixer
