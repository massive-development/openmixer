// RBJ-cookbook biquad filters (public formulas), stereo transposed direct form II.
#pragma once

namespace openmixer::dsp
{

struct BiquadCoeffs
{
    float b0 = 1.0F, b1 = 0.0F, b2 = 0.0F, a1 = 0.0F, a2 = 0.0F;
};

BiquadCoeffs designLowShelf (float sampleRate, float hz, float gainDb) noexcept;
BiquadCoeffs designHighShelf (float sampleRate, float hz, float gainDb) noexcept;
BiquadCoeffs designPeaking (float sampleRate, float hz, float gainDb, float q) noexcept;
BiquadCoeffs designLowPass (float sampleRate, float hz, float q) noexcept;
BiquadCoeffs designHighPass (float sampleRate, float hz, float q) noexcept;

class Biquad
{
public:
    void setCoeffs (const BiquadCoeffs& c) noexcept { k = c; }
    void reset() noexcept;
    void process (int channel, float* samples, int numSamples) noexcept;
    // Per-sample form for feedback loops (e.g. FX delay lines).
    float processSample (int channel, float in) noexcept
    {
        const float out = k.b0 * in + z1[channel];
        z1[channel] = k.b1 * in - k.a1 * out + z2[channel];
        z2[channel] = k.b2 * in - k.a2 * out;
        return out;
    }

private:
    BiquadCoeffs k;
    float z1[2] { 0.0F, 0.0F }, z2[2] { 0.0F, 0.0F };
};

} // namespace openmixer::dsp
