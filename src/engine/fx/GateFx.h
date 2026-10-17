// Trance gate: rhythmic on/off chopping at a beat division, with click-free edges.
#pragma once

#include "Effect.h"

namespace openmixer::fx
{

class GateFx final : public Effect
{
public:
    enum Param { kTime, kDuty };

    GateFx() noexcept;
    std::string_view getName() const noexcept override { return "gate"; }
    void reset() noexcept override;

protected:
    void prepareEffect (double sampleRate, int maxBlockSize) override;
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    double phase = 0.0;
    float envelope = 1.0F, attack = 1.0F, release = 1.0F;
};

} // namespace openmixer::fx
