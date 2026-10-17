#include "FlangerFx.h"

#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr double kMinDelaySec = 0.0005, kSweepSec = 0.004, kTwoPi = 6.283185307179586;
}

FlangerFx::FlangerFx() noexcept
    : Effect ({ { "time", 0.25F, 32.0F, 8.0F, 0.0 },
                { "depth", 0.0F, 1.0F, 0.7F },
                { "feedback", -0.9F, 0.9F, 0.5F } })
{
}

void FlangerFx::prepareEffect (double sampleRate, int)
{
    line.allocate (static_cast<int> (std::ceil ((kMinDelaySec + kSweepSec) * sampleRate)) + 4);
}

void FlangerFx::reset() noexcept
{
    line.reset();
    phase = 0.0;
    lastL = lastR = 0.0F;
}

float FlangerFx::delayFor (double ph, double sampleRate) const noexcept
{
    const double sweep = 0.5 - 0.5 * std::cos (kTwoPi * ph);
    return static_cast<float> ((kMinDelaySec + kSweepSec * param (kDepth) * sweep) * sampleRate);
}

void FlangerFx::render (float* left, float* right, int n, const FxContext& ctx) noexcept
{
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        phase += 1.0 / (static_cast<double> (param (kTime)) * ctx.samplesPerBeat);
        phase -= std::floor (phase);
        // Input scaled by (1 - |fb|) caps the resonant peak at unity gain.
        const float fb = param (kFeedback), inGain = 1.0F - std::abs (fb);
        line.push (flushDenormal (inGain * left[i] + fb * lastL), flushDenormal (inGain * right[i] + fb * lastR));
        lastL = line.read (0, delayFor (phase, ctx.sampleRate));
        lastR = line.read (1, delayFor (phase + 0.25, ctx.sampleRate));
        left[i] = 0.5F * (left[i] + lastL);
        right[i] = 0.5F * (right[i] + lastR);
    }
}

} // namespace openmixer::fx
