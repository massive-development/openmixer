#include "MeterBallistics.h"

#include "Theme.h"

#include <algorithm>
#include <array>
#include <cmath>

namespace openmixer::ui
{

namespace { constexpr float kSnapToZero = 0.002F; }

MeterParams MeterParams::fromTheme()
{
    return { theme::meterFloorDb(), theme::meterClipLevel(), theme::meterReleaseSeconds(),
             theme::meterHoldSeconds(), theme::meterClipSeconds() };
}

float MeterBallistics::proportionOf (float linear, float floorDb) noexcept
{
    if (linear <= 0.0F || floorDb >= 0.0F)
        return 0.0F;
    return std::clamp ((20.0F * std::log10 (linear) - floorDb) / -floorDb, 0.0F, 1.0F);
}

void MeterBallistics::updateClip (float linearPeak, float dt) noexcept
{
    if (linearPeak >= params.clipLevel)
        clipRemaining = params.clipSeconds;
    else
        clipRemaining = std::max (0.0F, clipRemaining - dt);
    clipping = linearPeak >= params.clipLevel || clipRemaining > 0.0F;
}

bool MeterBallistics::update (float linearPeak, float dt)
{
    const auto before = std::array<float, 2> { levelProportion, peakProportion };
    const bool clipBefore = clipping;

    const float target = proportionOf (linearPeak, params.floorDb);
    const float fall = std::exp (-std::max (dt, 0.0F) / std::max (params.releaseSeconds, 1e-3F));
    levelProportion = std::max (target, levelProportion * fall);
    if (levelProportion < kSnapToZero)
        levelProportion = 0.0F;

    if (target >= peakProportion)
    {
        peakProportion = target;
        holdRemaining = params.holdSeconds;
    }
    else if (holdRemaining > 0.0F)
        holdRemaining -= dt;
    else
        peakProportion = std::max (levelProportion, peakProportion * fall);

    updateClip (linearPeak, dt);
    return before[0] != levelProportion || before[1] != peakProportion || clipBefore != clipping;
}

} // namespace openmixer::ui
