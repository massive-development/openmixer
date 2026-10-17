#include "Quantizer.h"

#include <cmath>

namespace openmixer
{

namespace
{
    bool snappable (const BeatGrid* grid, double division) noexcept
    {
        return grid != nullptr && division > 0.0 && grid->getBpm() > 0.0;
    }
}

std::int64_t snapToNearest (const BeatGrid* grid, std::int64_t frame, double division) noexcept
{
    if (! snappable (grid, division))
        return frame;
    return grid->frameAtBeat (std::round (grid->beatAtFrame (frame) / division) * division);
}

std::int64_t snapToPrevious (const BeatGrid* grid, std::int64_t frame, double division) noexcept
{
    if (! snappable (grid, division))
        return frame;
    return grid->frameAtBeat (std::floor (grid->beatAtFrame (frame) / division) * division);
}

Loop makeQuantizedLoop (const BeatGrid* grid, std::int64_t startFrame, double lengthBeats, double division) noexcept
{
    if (grid == nullptr || lengthBeats <= 0.0)
        return {};
    const auto start = snapToNearest (grid, startFrame, division);
    return { start, grid->frameAtBeat (grid->beatAtFrame (start) + lengthBeats) };
}

} // namespace openmixer
