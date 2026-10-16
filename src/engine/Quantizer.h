// Snaps positions to a beat grid for quantized cue and loop. Pure functions, any thread.
// Without a grid (nullptr) or with division 0, positions pass through unchanged.
#pragma once

#include "BeatGrid.h"

namespace openmixer
{

struct Loop
{
    std::int64_t start = 0, end = 0;
    bool valid() const noexcept { return end > start; }
};

// division: grid step in beats (1 = beat, 0.5 = half beat, 4 = bar); 0 disables snapping.
std::int64_t snapToNearest (const BeatGrid* grid, std::int64_t frame, double division) noexcept;
std::int64_t snapToPrevious (const BeatGrid* grid, std::int64_t frame, double division) noexcept;

// Loop of lengthBeats starting on the grid step nearest to startFrame; invalid if length <= 0.
Loop makeQuantizedLoop (const BeatGrid* grid, std::int64_t startFrame, double lengthBeats, double division) noexcept;

} // namespace openmixer
