// Fixed-capacity ring of stereo source frames tagged with their source index.
// Single-thread (audio thread); allocates only on construction; pushing when full drops the oldest frame.
#pragma once

#include <algorithm>
#include <cstddef>
#include <juce_core/juce_core.h>
#include <vector>

namespace openmixer
{

class FrameRing
{
public:
    explicit FrameRing (size_t capacity) : frames (capacity * 2), indices (capacity) {}

    size_t size() const noexcept { return count; }
    size_t capacity() const noexcept { return indices.size(); }
    void clear() noexcept { head = count = 0; }

    void push (const float* f, juce::int64 index) noexcept
    {
        if (count == capacity())
            dropOldest();
        const auto slot = (head + count) % capacity();
        frames[slot * 2] = f[0];
        frames[slot * 2 + 1] = f[1];
        indices[slot] = index;
        ++count;
    }

    bool pop (float* f, juce::int64& index) noexcept
    {
        if (count == 0)
            return false;
        f[0] = frames[head * 2];
        f[1] = frames[head * 2 + 1];
        index = indices[head];
        dropOldest();
        return true;
    }

    // Replaces dst's contents with the newest n frames, oldest first.
    void copyNewestTo (FrameRing& dst, size_t n) const noexcept
    {
        dst.clear();
        n = std::min (n, count);
        for (size_t i = count - n; i < count; ++i)
        {
            const auto slot = (head + i) % capacity();
            dst.push (&frames[slot * 2], indices[slot]);
        }
    }

private:
    void dropOldest() noexcept { head = (head + 1) % capacity(); --count; }

    std::vector<float> frames;
    std::vector<juce::int64> indices;
    size_t head = 0, count = 0;
};

} // namespace openmixer
