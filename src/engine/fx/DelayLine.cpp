#include "DelayLine.h"

#include <algorithm>
#include <cmath>

namespace openmixer::fx
{

void DelayLine::allocate (int maxDelaySamples)
{
    size = std::max (4, maxDelaySamples + 2);
    for (auto& b : buffers)
        b.assign (static_cast<size_t> (size), 0.0F);
    writePos = 0;
    wipe.finish (size);
}

void DelayLine::reset() noexcept
{
    for (auto& b : buffers)
        std::fill (b.begin(), b.end(), 0.0F);
    writePos = 0;
    wipe.finish (size);
}

void DelayLine::beginClear() noexcept
{
    writePos = 0;
    wipe.begin();
}

bool DelayLine::clearSome (int maxFrames) noexcept
{
    int from = 0;
    const int count = wipe.claim (size, maxFrames, from);
    for (auto& b : buffers)
        std::fill_n (b.begin() + from, count, 0.0F);
    return wipe.done (size);
}

void DelayLine::push (float left, float right) noexcept
{
    if (size == 0)
        return;
    const auto i = static_cast<size_t> (writePos);
    buffers[0][i] = left;
    buffers[1][i] = right;
    wipe.wrote (writePos);
    writePos = writePos + 1 == size ? 0 : writePos + 1;
}

float DelayLine::read (int channel, float delaySamples) const noexcept
{
    if (size == 0)
        return 0.0F;
    const float d = std::clamp (delaySamples, 1.0F, static_cast<float> (size - 2));
    const auto whole = static_cast<int> (std::floor (d));
    const float frac = d - static_cast<float> (whole);
    const auto& b = buffers[static_cast<size_t> (channel & 1)];
    const auto at = [&] (int back)
    {
        const int slot = (writePos - back + size) % size;
        return wipe.live (slot) ? b[static_cast<size_t> (slot)] : 0.0F;
    };
    return at (whole) + frac * (at (whole + 1) - at (whole));
}

} // namespace openmixer::fx
