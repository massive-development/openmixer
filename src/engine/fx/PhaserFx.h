// Phaser: six swept first-order allpass stages with feedback; sweep period in beats.
#pragma once

#include "Effect.h"

#include <array>

namespace openmixer::fx
{

class PhaserFx final : public Effect
{
public:
    enum Param { kTime, kDepth, kFeedback };
    static constexpr int kStages = 6;

    PhaserFx() noexcept;
    std::string_view getName() const noexcept override { return "phaser"; }
    void reset() noexcept override;

protected:
    void prepareEffect (double, int) override {}
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    float coefficientFor (double phase, double sampleRate) const noexcept;
    float runStages (int channel, float in, float coeff) noexcept;

    std::array<std::array<float, kStages>, 2> state {};
    std::array<float, 2> last {};
    double phase = 0.0;
};

} // namespace openmixer::fx
