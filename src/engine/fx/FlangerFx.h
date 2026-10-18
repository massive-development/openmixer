// Flanger: short modulated delay with feedback; the sweep period is in beats.
#pragma once

#include "DelayLine.h"
#include "Effect.h"

namespace openmixer::fx
{

class FlangerFx final : public Effect
{
public:
    enum Param { kTime, kDepth, kFeedback };

    FlangerFx() noexcept;
    std::string_view getName() const noexcept override { return "flanger"; }
    void reset() noexcept override;
    void beginReset() noexcept override;
    bool clearSome (int maxFrames) noexcept override { return line.clearSome (maxFrames); }

protected:
    void prepareEffect (double sampleRate, int maxBlockSize) override;
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    float delayFor (double phase, double sampleRate) const noexcept;

    DelayLine line;
    double phase = 0.0;
    float lastL = 0.0F, lastR = 0.0F;
};

} // namespace openmixer::fx
