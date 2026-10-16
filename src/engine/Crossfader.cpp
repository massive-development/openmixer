#include "Crossfader.h"

#include <algorithm>
#include <cmath>

namespace openmixer
{

namespace { constexpr float kHalfPi = 1.57079632679F; }

void Crossfader::setPosition (float p) noexcept
{
    position.store (std::clamp (p, 0.0F, 1.0F), std::memory_order_relaxed);
}

CrossfaderGains Crossfader::gains() const noexcept
{
    const float p = position.load (std::memory_order_relaxed);
    if (curve.load (std::memory_order_relaxed) == CrossfaderCurve::Linear)
        return { 1.0F - p, p };
    return { std::cos (p * kHalfPi), std::sin (p * kHalfPi) };
}

float Crossfader::gainFor (const CrossfaderGains& g, CrossfaderAssign a) noexcept
{
    switch (a)
    {
        case CrossfaderAssign::Left:  return g.left;
        case CrossfaderAssign::Right: return g.right;
        case CrossfaderAssign::Thru:  break;
    }
    return 1.0F;
}

} // namespace openmixer
