// Beat-synced echo: filtered feedback delay (time in beats).
#pragma once

#include "DelayLine.h"
#include "Effect.h"
#include "dsp/Biquad.h"

namespace openmixer::fx
{

class EchoFx final : public Effect
{
public:
    enum Param { kTime, kFeedback, kTone };
    static constexpr float kMaxBeats = 4.0F;

    EchoFx() noexcept;
    std::string_view getName() const noexcept override { return "echo"; }
    bool hasTail() const noexcept override { return true; }
    int tailSilenceWindow (const FxContext& ctx) const noexcept override;
    void reset() noexcept override;

protected:
    void prepareEffect (double sampleRate, int maxBlockSize) override;
    void render (float* left, float* right, int numSamples, const FxContext& ctx) noexcept override;

private:
    void updateTone (double sampleRate) noexcept;

    DelayLine line;
    dsp::Biquad toneFilter, lowCut;
};

} // namespace openmixer::fx
