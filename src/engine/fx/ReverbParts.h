// Building blocks for the reverb: damped feedback comb and Schroeder allpass (textbook forms).
#pragma once

#include "FxMath.h"
#include "Wipe.h"

#include <algorithm>
#include <vector>

namespace openmixer::fx
{

inline int wipeSlots (std::vector<float>& buffer, Wipe& wipe, int maxFrames) noexcept
{
    int from = 0;
    const int count = wipe.claim (static_cast<int> (buffer.size()), maxFrames, from);
    std::fill_n (buffer.begin() + from, count, 0.0F);
    return count;
}

class DampedComb
{
public:
    void allocate (int length) { buffer.assign (static_cast<size_t> (std::max (1, length)), 0.0F); pos = 0; wipe.finish (size()); }
    void reset() noexcept { std::fill (buffer.begin(), buffer.end(), 0.0F); pos = 0; lowState = 0.0F; wipe.finish (size()); }
    void beginClear() noexcept { pos = 0; lowState = 0.0F; wipe.begin(); }
    // Zeroes up to maxFrames stale slots; returns how many were used.
    int clearSome (int maxFrames) noexcept { return wipeSlots (buffer, wipe, maxFrames); }
    bool clean() const noexcept { return wipe.done (size()); }

    float process (float in, float feedback, float damp) noexcept
    {
        auto& slot = buffer[static_cast<size_t> (pos)];
        const float out = wipe.live (pos) ? slot : 0.0F;
        lowState = flushDenormal (out + damp * (lowState - out));
        slot = flushDenormal (in + feedback * lowState);
        wipe.wrote (pos);
        pos = pos + 1 == static_cast<int> (buffer.size()) ? 0 : pos + 1;
        return out;
    }

private:
    int size() const noexcept { return static_cast<int> (buffer.size()); }

    std::vector<float> buffer;
    Wipe wipe;
    int pos = 0;
    float lowState = 0.0F;
};

class SchroederAllpass
{
public:
    void allocate (int length) { buffer.assign (static_cast<size_t> (std::max (1, length)), 0.0F); pos = 0; wipe.finish (size()); }
    void reset() noexcept { std::fill (buffer.begin(), buffer.end(), 0.0F); pos = 0; wipe.finish (size()); }
    void beginClear() noexcept { pos = 0; wipe.begin(); }
    int clearSome (int maxFrames) noexcept { return wipeSlots (buffer, wipe, maxFrames); }
    bool clean() const noexcept { return wipe.done (size()); }

    float process (float in) noexcept
    {
        constexpr float g = 0.5F;
        auto& slot = buffer[static_cast<size_t> (pos)];
        const float delayed = wipe.live (pos) ? slot : 0.0F;
        slot = flushDenormal (in + g * delayed);
        wipe.wrote (pos);
        pos = pos + 1 == static_cast<int> (buffer.size()) ? 0 : pos + 1;
        return delayed - g * slot;
    }

private:
    int size() const noexcept { return static_cast<int> (buffer.size()); }

    std::vector<float> buffer;
    Wipe wipe;
    int pos = 0;
};

} // namespace openmixer::fx
