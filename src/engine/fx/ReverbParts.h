// Building blocks for the reverb: damped feedback comb and Schroeder allpass (textbook forms).
#pragma once

#include "FxMath.h"

#include <algorithm>
#include <vector>

namespace openmixer::fx
{

class DampedComb
{
public:
    void allocate (int length) { buffer.assign (static_cast<size_t> (std::max (1, length)), 0.0F); pos = 0; }
    void reset() noexcept { std::fill (buffer.begin(), buffer.end(), 0.0F); pos = 0; lowState = 0.0F; }

    float process (float in, float feedback, float damp) noexcept
    {
        auto& slot = buffer[static_cast<size_t> (pos)];
        const float out = slot;
        lowState = flushDenormal (out + damp * (lowState - out));
        slot = flushDenormal (in + feedback * lowState);
        pos = pos + 1 == static_cast<int> (buffer.size()) ? 0 : pos + 1;
        return out;
    }

private:
    std::vector<float> buffer;
    int pos = 0;
    float lowState = 0.0F;
};

class SchroederAllpass
{
public:
    void allocate (int length) { buffer.assign (static_cast<size_t> (std::max (1, length)), 0.0F); pos = 0; }
    void reset() noexcept { std::fill (buffer.begin(), buffer.end(), 0.0F); pos = 0; }

    float process (float in) noexcept
    {
        constexpr float g = 0.5F;
        auto& slot = buffer[static_cast<size_t> (pos)];
        const float delayed = slot;
        slot = flushDenormal (in + g * delayed);
        pos = pos + 1 == static_cast<int> (buffer.size()) ? 0 : pos + 1;
        return delayed - g * slot;
    }

private:
    std::vector<float> buffer;
    int pos = 0;
};

} // namespace openmixer::fx
