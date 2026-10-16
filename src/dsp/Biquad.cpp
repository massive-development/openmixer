#include "Biquad.h"

#include <cmath>

namespace openmixer::dsp
{

namespace
{
    constexpr float kPi = 3.14159265358979F;
    constexpr float kShelfQ = 0.70710678F;

    BiquadCoeffs normalised (float b0, float b1, float b2, float a0, float a1, float a2) noexcept
    {
        return { b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0 };
    }

    struct Angle
    {
        float cs, al;
        Angle (float sampleRate, float hz, float q) noexcept
            : cs (std::cos (2.0F * kPi * hz / sampleRate)),
              al (std::sin (2.0F * kPi * hz / sampleRate) / (2.0F * q)) {}
    };
}

BiquadCoeffs designLowShelf (float sr, float hz, float gainDb) noexcept
{
    const float A = std::pow (10.0F, gainDb / 40.0F);
    const Angle w (sr, hz, kShelfQ);
    const float t = 2.0F * std::sqrt (A) * w.al;
    return normalised (A * ((A + 1) - (A - 1) * w.cs + t), 2 * A * ((A - 1) - (A + 1) * w.cs),
                       A * ((A + 1) - (A - 1) * w.cs - t),
                       (A + 1) + (A - 1) * w.cs + t, -2 * ((A - 1) + (A + 1) * w.cs), (A + 1) + (A - 1) * w.cs - t);
}

BiquadCoeffs designHighShelf (float sr, float hz, float gainDb) noexcept
{
    const float A = std::pow (10.0F, gainDb / 40.0F);
    const Angle w (sr, hz, kShelfQ);
    const float t = 2.0F * std::sqrt (A) * w.al;
    return normalised (A * ((A + 1) + (A - 1) * w.cs + t), -2 * A * ((A - 1) + (A + 1) * w.cs),
                       A * ((A + 1) + (A - 1) * w.cs - t),
                       (A + 1) - (A - 1) * w.cs + t, 2 * ((A - 1) - (A + 1) * w.cs), (A + 1) - (A - 1) * w.cs - t);
}

BiquadCoeffs designPeaking (float sr, float hz, float gainDb, float q) noexcept
{
    const float A = std::pow (10.0F, gainDb / 40.0F);
    const Angle w (sr, hz, q);
    return normalised (1 + w.al * A, -2 * w.cs, 1 - w.al * A, 1 + w.al / A, -2 * w.cs, 1 - w.al / A);
}

BiquadCoeffs designLowPass (float sr, float hz, float q) noexcept
{
    const Angle w (sr, hz, q);
    return normalised ((1 - w.cs) / 2, 1 - w.cs, (1 - w.cs) / 2, 1 + w.al, -2 * w.cs, 1 - w.al);
}

BiquadCoeffs designHighPass (float sr, float hz, float q) noexcept
{
    const Angle w (sr, hz, q);
    return normalised ((1 + w.cs) / 2, -(1 + w.cs), (1 + w.cs) / 2, 1 + w.al, -2 * w.cs, 1 - w.al);
}

void Biquad::reset() noexcept
{
    z1[0] = z1[1] = z2[0] = z2[1] = 0.0F;
}

void Biquad::process (int c, float* x, int n) noexcept
{
    float s1 = z1[c], s2 = z2[c];
    for (int i = 0; i < n; ++i)
    {
        const float in = x[i];
        const float out = k.b0 * in + s1;
        s1 = k.b1 * in - k.a1 * out + s2;
        s2 = k.b2 * in - k.a2 * out;
        x[i] = out;
    }
    z1[c] = s1;
    z2[c] = s2;
}

} // namespace openmixer::dsp
