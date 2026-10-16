#include "SweepFilter.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace
{
    constexpr float kDeadZone = 0.02F, kQ = 0.7071F, kMinLowPassHz = 40.0F, kMinHighPassHz = 20.0F;
    constexpr float kForceRefresh = 1000.0F;
}

void SweepFilter::setPosition (float v) noexcept
{
    position.store (std::clamp (v, -1.0F, 1.0F), std::memory_order_relaxed);
}

void SweepFilter::prepare (double sr, int)
{
    sampleRate = static_cast<float> (sr);
    applied = kForceRefresh;
    filter.reset();
    refreshCoefficients();
}

void SweepFilter::refreshCoefficients() noexcept
{
    const float v = getPosition();
    if (v == applied)
        return;
    applied = v;
    active = std::abs (v) > kDeadZone;
    if (! active)
        return;

    const float maxHz = std::min (20000.0F, sampleRate * 0.45F);
    if (v < 0.0F)
        filter.setCoeffs (dsp::designLowPass (sampleRate, maxHz * std::pow (kMinLowPassHz / maxHz, -v), kQ));
    else
        filter.setCoeffs (dsp::designHighPass (sampleRate, kMinHighPassHz * std::pow (maxHz / kMinHighPassHz, v), kQ));
}

void SweepFilter::process (float* left, float* right, int n) noexcept
{
    refreshCoefficients();
    if (! active)
        return;
    filter.process (0, left, n);
    filter.process (1, right, n);
}

} // namespace openmixer
