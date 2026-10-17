#include "TapeStopFx.h"

#include <algorithm>
#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr double kSilenceBelowSpeed = 0.125;   // fade to silence over the last part of the stop
}

TapeStopFx::TapeStopFx() noexcept
    : Effect ({ { "time", 0.25F, kMaxBeats, 1.0F, 0.0 } })
{
}

void TapeStopFx::prepareEffect (double sampleRate, int)
{
    // The read head lags the write head by at most the stop length.
    const auto size = static_cast<size_t> (std::ceil (kMaxBeats * sampleRate * 60.0 / kMinBpm)) + 4;
    for (auto& ch : history)
        ch.assign (size, 0.0F);
}

void TapeStopFx::reset() noexcept
{
    written = 0;
    readPos = 0.0;
    speed = 1.0;
}

float TapeStopFx::readAt (int channel, double position) const noexcept
{
    const auto& h = history[static_cast<size_t> (channel)];
    const auto size = static_cast<std::int64_t> (h.size());
    const auto whole = static_cast<std::int64_t> (std::floor (position));
    const auto frac = static_cast<float> (position - static_cast<double> (whole));
    const auto next = std::min (whole + 1, written - 1);
    const float a = h[static_cast<size_t> (whole % size)], b = h[static_cast<size_t> (next % size)];
    return a + frac * (b - a);
}

void TapeStopFx::render (float* left, float* right, int n, const FxContext& ctx) noexcept
{
    if (history[0].empty())
        return;
    const auto size = static_cast<std::int64_t> (history[0].size());
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        const auto slot = static_cast<size_t> (written % size);
        history[0][slot] = left[i];
        history[1][slot] = right[i];
        ++written;
        const auto gain = static_cast<float> (std::min (1.0, speed / kSilenceBelowSpeed));
        left[i] = readAt (0, readPos) * gain;
        right[i] = readAt (1, readPos) * gain;
        readPos += speed;
        speed = std::max (0.0, speed - 1.0 / (static_cast<double> (param (kTime)) * ctx.samplesPerBeat));
    }
}

} // namespace openmixer::fx
