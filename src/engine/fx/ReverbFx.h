// Stereo room reverb: parallel damped combs into series allpasses (Schroeder/Moorer topology).
#pragma once

#include "Effect.h"
#include "ReverbParts.h"

#include <array>

namespace openmixer::fx
{

class ReverbFx final : public Effect
{
public:
    enum Param { kSize, kDamp, kWidth };
    static constexpr int kNumCombs = 8, kNumAllpasses = 4;

    ReverbFx() noexcept;
    std::string_view getName() const noexcept override { return "reverb"; }
    bool hasTail() const noexcept override { return true; }
    int tailSilenceWindow (const FxContext& ctx) const noexcept override;
    void reset() noexcept override;

protected:
    void prepareEffect (double sampleRate, int maxBlockSize) override;
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    float processChannel (int channel, float in, float feedback, float damp) noexcept;

    std::array<std::array<DampedComb, kNumCombs>, 2> combs;
    std::array<std::array<SchroederAllpass, kNumAllpasses>, 2> allpasses;
};

} // namespace openmixer::fx
