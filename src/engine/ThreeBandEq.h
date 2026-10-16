// Low shelf 250 Hz, mid peak 1 kHz, high shelf 4 kHz; -48 dB acts as a kill.
#pragma once

#include "ProcessStage.h"
#include "../dsp/Biquad.h"

#include <array>
#include <atomic>

namespace openmixer
{

class ThreeBandEq : public ProcessStage
{
public:
    static constexpr float kMinDb = -48.0F;
    static constexpr float kMaxDb = 6.0F;

    void setGainDb (int band, float dB) noexcept;
    float getGainDb (int band) const noexcept;

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (float* left, float* right, int numSamples) noexcept override;

private:
    void refreshCoefficients() noexcept;

    std::array<std::atomic<float>, 3> gains {};
    std::array<float, 3> applied { 0.0F, 0.0F, 0.0F };
    std::array<dsp::Biquad, 3> bands;
    float sampleRate = 44100.0F;
};

} // namespace openmixer
