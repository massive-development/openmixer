// One-knob DJ filter: -1 low-pass, 0 off, +1 high-pass.
#pragma once

#include "ProcessStage.h"
#include "../dsp/Biquad.h"

#include <atomic>

namespace openmixer
{

class SweepFilter : public ProcessStage
{
public:
    void setPosition (float v) noexcept;
    float getPosition() const noexcept { return position.load (std::memory_order_relaxed); }

    void prepare (double sampleRate, int maxBlockSize) override;
    void process (float* left, float* right, int numSamples) noexcept override;

private:
    void refreshCoefficients() noexcept;

    std::atomic<float> position { 0.0F };
    float applied = 0.0F;
    bool active = false;
    float sampleRate = 44100.0F;
    dsp::Biquad filter;
};

} // namespace openmixer
