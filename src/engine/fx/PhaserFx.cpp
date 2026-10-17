#include "PhaserFx.h"

#include <algorithm>
#include <cmath>

namespace openmixer::fx
{

namespace
{
    constexpr double kLowHz = 250.0, kHighHz = 4000.0, kPi = 3.141592653589793;
}

PhaserFx::PhaserFx() noexcept
    : Effect ({ { "time", 0.25F, 32.0F, 4.0F, 0.0 },
                { "depth", 0.0F, 1.0F, 0.8F },
                { "feedback", 0.0F, 0.9F, 0.4F } })
{
}

void PhaserFx::reset() noexcept
{
    for (auto& ch : state)
        ch.fill (0.0F);
    last.fill (0.0F);
    phase = 0.0;
}

float PhaserFx::coefficientFor (double ph, double sampleRate) const noexcept
{
    const double sweep = (0.5 - 0.5 * std::cos (2.0 * kPi * ph)) * static_cast<double> (param (kDepth));
    const double hz = std::min (kLowHz * std::pow (kHighHz / kLowHz, sweep), sampleRate * 0.45);
    const double t = std::tan (kPi * hz / sampleRate);
    return static_cast<float> ((t - 1.0) / (t + 1.0));
}

float PhaserFx::runStages (int channel, float in, float a) noexcept
{
    auto& z = state[static_cast<size_t> (channel)];
    float x = in;
    for (auto& s : z)
    {
        const float y = a * x + s;
        s = flushDenormal (x - a * y);
        x = y;
    }
    return x;
}

void PhaserFx::render (float* left, float* right, int n, const FxContext& ctx) noexcept
{
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        phase += 1.0 / (static_cast<double> (param (kTime)) * ctx.samplesPerBeat);
        phase -= std::floor (phase);
        const float fb = param (kFeedback), inGain = 1.0F - fb;
        const std::array<float*, 2> io { left + i, right + i };
        for (int c = 0; c < 2; ++c)
        {
            const auto cs = static_cast<size_t> (c);
            const float a = coefficientFor (phase + 0.25 * c, ctx.sampleRate);
            last[cs] = runStages (c, inGain * *io[cs] + fb * last[cs], a);
            *io[cs] = 0.5F * (*io[cs] + last[cs]);
        }
    }
}

} // namespace openmixer::fx
