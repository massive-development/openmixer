#include "RollFx.h"

#include <algorithm>
#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr int kEdgeFade = 96;
}

RollFx::RollFx() noexcept
    : Effect ({ { "time", 1.0F / 16.0F, kMaxBeats, 0.5F, 0.0 } })
{
}

void RollFx::prepareEffect (double sampleRate, int)
{
    capacity = static_cast<int> (std::ceil (kMaxBeats * sampleRate * 60.0 / kMinBpm));
    for (auto& ch : loop)
        ch.assign (static_cast<size_t> (capacity), 0.0F);
}

void RollFx::reset() noexcept
{
    recorded = 0;
    playPos = 0;
}

float RollFx::edgeGain (int pos, int length, bool fadeIn) noexcept
{
    // Short fades at the loop seam avoid a click on every repeat.
    const int fade = std::min (kEdgeFade, length / 4);
    if (fade <= 0)
        return 1.0F;
    const int edge = fadeIn ? std::min (pos, length - 1 - pos) : length - 1 - pos;
    return std::min (1.0F, static_cast<float> (edge) / static_cast<float> (fade));
}

void RollFx::render (float* left, float* right, int n, const FxContext& ctx) noexcept
{
    if (capacity == 0)
        return;
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        const int length = std::clamp (static_cast<int> (param (kTime) * ctx.samplesPerBeat), 1, capacity);
        if (recorded < length)
        {
            loop[0][static_cast<size_t> (recorded)] = left[i];
            loop[1][static_cast<size_t> (recorded)] = right[i];
            const float g = edgeGain (recorded, length, false);   // live pass: fade out only
            left[i] *= g;
            right[i] *= g;
            ++recorded;
            continue;
        }
        playPos %= length;
        const float g = edgeGain (playPos, length, true);
        left[i] = loop[0][static_cast<size_t> (playPos)] * g;
        right[i] = loop[1][static_cast<size_t> (playPos)] * g;
        ++playPos;
    }
}

} // namespace openmixer::fx
