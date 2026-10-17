// Stereo circular buffer with fractional (linear) reads; sized on the control thread.
#pragma once

#include <array>
#include <vector>

namespace openmixer::fx
{

class DelayLine
{
public:
    // Control thread (prepare): allocates; never called from process().
    void allocate (int maxDelaySamples);
    void reset() noexcept;

    void push (float left, float right) noexcept;
    // delaySamples is clamped to [1, capacity - 2]; 1 = the most recent push.
    float read (int channel, float delaySamples) const noexcept;
    int capacity() const noexcept { return size; }

private:
    std::array<std::vector<float>, 2> buffers;
    int writePos = 0;
    int size = 0;
};

} // namespace openmixer::fx
