#include "ThreeBandEq.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    constexpr float kLowHz = 250.0F, kMidHz = 1000.0F, kHighHz = 4000.0F, kMidQ = 0.7071F;
    constexpr float kForceRefresh = 1000.0F;
    constexpr auto kRelaxed = std::memory_order_relaxed;
}

void ThreeBandEq::setGainDb (int band, float dB) noexcept
{
    gains[static_cast<size_t> (std::clamp (band, 0, 2))].store (std::clamp (dB, kMinDb, kMaxDb), kRelaxed);
}

float ThreeBandEq::getGainDb (int band) const noexcept
{
    return gains[static_cast<size_t> (std::clamp (band, 0, 2))].load (kRelaxed);
}

void ThreeBandEq::prepare (double sr, int)
{
    sampleRate = static_cast<float> (sr);
    applied = { kForceRefresh, kForceRefresh, kForceRefresh };
    for (auto& b : bands)
        b.reset();
    refreshCoefficients();
}

void ThreeBandEq::refreshCoefficients() noexcept
{
    const std::array<float, 3> now { getGainDb (0), getGainDb (1), getGainDb (2) };
    if (now[0] != applied[0]) bands[0].setCoeffs (dsp::designLowShelf (sampleRate, kLowHz, now[0]));
    if (now[1] != applied[1]) bands[1].setCoeffs (dsp::designPeaking (sampleRate, kMidHz, now[1], kMidQ));
    if (now[2] != applied[2]) bands[2].setCoeffs (dsp::designHighShelf (sampleRate, kHighHz, now[2]));
    applied = now;
}

void ThreeBandEq::process (float* left, float* right, int n) noexcept
{
    refreshCoefficients();
    for (auto& band : bands)
    {
        band.process (0, left, n);
        band.process (1, right, n);
    }
}

} // namespace openmixer
