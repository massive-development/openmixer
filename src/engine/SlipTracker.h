// Slip mode bookkeeping: while a loop/scratch/hold runs, a ghost playhead keeps moving at the
// underlying speed so playback can resume where the track would have been. Control thread.
#pragma once

#include <algorithm>
#include <cstdint>

namespace openmixer
{

class SlipTracker
{
public:
    void begin (std::int64_t sourceFrame) noexcept { ghost = static_cast<double> (sourceFrame); active = true; }

    // Source frames the track would have played since the last call (elapsed time x rate).
    void advance (double sourceFrames) noexcept { if (active) ghost += sourceFrames; }

    // Returns the resume position clamped to the track, or -1 when slip was not active.
    std::int64_t end (std::int64_t lengthFrames) noexcept
    {
        if (! active)
            return -1;
        active = false;
        return std::clamp (static_cast<std::int64_t> (ghost), std::int64_t { 0 }, std::max<std::int64_t> (lengthFrames - 1, 0));
    }

    bool isActive() const noexcept { return active; }

private:
    double ghost = 0.0;
    bool active = false;
};

} // namespace openmixer
