#include "CrushFx.h"

#include <cmath>

namespace openmixer::fx
{

CrushFx::CrushFx() noexcept
    : Effect ({ { "bits", 2.0F, 16.0F, 8.0F }, { "downsample", 1.0F, 32.0F, 4.0F } })
{
}

void CrushFx::reset() noexcept
{
    held.fill (0.0F);
    holdPhase = 1.0F;
}

void CrushFx::render (float* left, float* right, int n, const FxContext&) noexcept
{
    for (int i = 0; i < n; ++i)
    {
        tickParams();
        holdPhase += 1.0F / param (kDownsample);
        if (holdPhase >= 1.0F)
        {
            holdPhase -= std::floor (holdPhase);
            const float step = std::exp2 (1.0F - param (kBits));
            held[0] = std::round (left[i] / step) * step;
            held[1] = std::round (right[i] / step) * step;
        }
        left[i] = held[0];
        right[i] = held[1];
    }
}

} // namespace openmixer::fx
