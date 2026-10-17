// Bit crusher: amplitude quantisation plus sample-rate reduction (sample-and-hold).
#pragma once

#include "Effect.h"

#include <array>

namespace openmixer::fx
{

class CrushFx final : public Effect
{
public:
    enum Param { kBits, kDownsample };

    CrushFx() noexcept;
    std::string_view getName() const noexcept override { return "crush"; }
    void reset() noexcept override;

protected:
    void prepareEffect (double, int) override {}
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    std::array<float, 2> held {};
    float holdPhase = 1.0F;
};

} // namespace openmixer::fx
