// Decaying peak level written by the audio thread, read by anyone.
#pragma once

#include <algorithm>
#include <atomic>

namespace openmixer
{

class PeakMeter
{
public:
    void push (float blockPeak, float decay) noexcept
    {
        value.store (std::max (blockPeak, value.load (std::memory_order_relaxed) * decay), std::memory_order_relaxed);
    }
    void reset() noexcept { value.store (0.0F, std::memory_order_relaxed); }
    float get() const noexcept { return value.load (std::memory_order_relaxed); }

private:
    std::atomic<float> value { 0.0F };
};

} // namespace openmixer
