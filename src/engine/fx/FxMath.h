// Small realtime helpers shared by the FX: parameter smoothing and denormal flushing.
#pragma once

#include <cmath>

namespace openmixer::fx
{

// Tiny feedback values become denormals on some CPUs; snapping them keeps the cost flat.
inline float flushDenormal (float x) noexcept
{
    return std::abs (x) < 1.0e-15F ? 0.0F : x;
}

// One-pole smoother; a zero time constant makes it follow the target instantly.
class Smoother
{
public:
    void setTime (double sampleRate, double seconds) noexcept
    {
        coeff = seconds <= 0.0 ? 1.0F : static_cast<float> (1.0 - std::exp (-1.0 / (seconds * sampleRate)));
    }
    void snap (float v) noexcept { value = v; }
    float current() const noexcept { return value; }

    float step (float target) noexcept
    {
        value += coeff * (target - value);
        if (std::abs (target - value) < 1.0e-6F)
            value = target;
        return value;
    }

private:
    float coeff = 1.0F;
    float value = 0.0F;
};

} // namespace openmixer::fx
