#include "EchoFx.h"

#include <algorithm>
#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr float kQ = 0.7071F, kLowCutHz = 70.0F, kMinToneHz = 600.0F;
}

EchoFx::EchoFx() noexcept
    : Effect ({ { "time", 1.0F / 16.0F, kMaxBeats, 0.75F, 0.15 },
                { "feedback", 0.0F, 0.95F, 0.45F },
                { "tone", 0.0F, 1.0F, 0.6F } })
{
}

void EchoFx::prepareEffect (double sampleRate, int)
{
    line.allocate (static_cast<int> (std::ceil (kMaxBeats * sampleRate * 60.0 / kMinBpm)));
    lowCut.setCoeffs (dsp::designHighPass (static_cast<float> (sampleRate), kLowCutHz, kQ));
}

void EchoFx::reset() noexcept
{
    line.reset();
    toneFilter.reset();
    lowCut.reset();
}

int EchoFx::tailSilenceWindow (const FxContext& ctx) const noexcept
{
    return static_cast<int> (getParamTarget (kTime) * ctx.samplesPerBeat) + 64;
}

void EchoFx::updateTone (double sampleRate) noexcept
{
    const auto sr = static_cast<float> (sampleRate);
    const float maxHz = std::min (18000.0F, sr * 0.45F);
    toneFilter.setCoeffs (dsp::designLowPass (sr, kMinToneHz * std::pow (maxHz / kMinToneHz, param (kTone)), kQ));
}

void EchoFx::render (float* left, float* right, int n, const FxContext& ctx) noexcept
{
    updateTone (ctx.sampleRate);
    const auto spb = static_cast<float> (ctx.samplesPerBeat);
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        const float d = param (kTime) * spb, fb = param (kFeedback);
        const float yl = line.read (0, d), yr = line.read (1, d);
        const float fl = lowCut.processSample (0, toneFilter.processSample (0, yl));
        const float fr = lowCut.processSample (1, toneFilter.processSample (1, yr));
        line.push (flushDenormal (left[i] + fb * fl), flushDenormal (right[i] + fb * fr));
        left[i] = yl;
        right[i] = yr;
    }
}

} // namespace openmixer::fx
