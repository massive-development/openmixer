// Sliced buffer wipe for ring buffers: slots below `valid` hold real (or zero) data, the rest are
// stale and must read as zero. Lets an effect be engaged while its buffer is still being wiped, so
// no audio callback clears more than (blockSize / 64) * kSlice frames per unit. Audio thread only.
#pragma once

#include <algorithm>

namespace openmixer::fx
{

class Wipe
{
public:
    static constexpr int kSlice = 512;   // max frames wiped per 64-sample chunk (per effect, all buffers)

    void finish (int size) noexcept { valid = size; }   // buffer is fully clean
    void begin() noexcept { valid = 0; }                // everything is stale
    bool live (int slot) const noexcept { return slot < valid; }
    void wrote (int slot) noexcept { if (slot == valid) ++valid; }   // writes advance the frontier
    bool done (int size) const noexcept { return valid >= size; }

    // Claims up to maxFrames stale slots; the caller zeroes [from, from + count).
    int claim (int size, int maxFrames, int& from) noexcept
    {
        from = valid;
        const int count = std::clamp (size - valid, 0, std::max (0, maxFrames));
        valid += count;
        return count;
    }

private:
    int valid = 0;
};

} // namespace openmixer::fx
