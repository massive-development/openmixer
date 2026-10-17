#include "GateFx.h"

#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr double kAttackSec = 0.001, kReleaseSec = 0.004;

    float onePoleCoeff (double seconds, double sampleRate) noexcept
    {
        return static_cast<float> (1.0 - std::exp (-1.0 / (seconds * sampleRate)));
    }
}

GateFx::GateFx() noexcept
    : Effect ({ { "time", 1.0F / 16.0F, 2.0F, 0.25F, 0.0 }, { "duty", 0.1F, 0.9F, 0.5F } })
{
}

void GateFx::prepareEffect (double sampleRate, int)
{
    attack = onePoleCoeff (kAttackSec, sampleRate);
    release = onePoleCoeff (kReleaseSec, sampleRate);
}

void GateFx::reset() noexcept
{
    phase = 0.0;
    envelope = 1.0F;
}

void GateFx::render (float* left, float* right, int n, const FxContext& ctx) noexcept
{
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        const float open = phase < static_cast<double> (param (kDuty)) ? 1.0F : 0.0F;
        envelope = flushDenormal (envelope + (open > envelope ? attack : release) * (open - envelope));
        phase += 1.0 / (static_cast<double> (param (kTime)) * ctx.samplesPerBeat);
        phase -= std::floor (phase);
        left[i] *= envelope;
        right[i] *= envelope;
    }
}

} // namespace openmixer::fx
