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
    // Audio thread: restart at the write head and clear the buffers in slices (no big memset).
    void beginClear() noexcept;
    // Clears up to maxFrames of what beginClear() left; true once nothing is pending.
    bool clearSome (int maxFrames) noexcept;

    void push (float left, float right) noexcept;
    // delaySamples is clamped to [1, capacity - 2]; 1 = the most recent push.
    float read (int channel, float delaySamples) const noexcept;
    int capacity() const noexcept { return size; }

private:
    std::array<std::vector<float>, 2> buffers;
    int writePos = 0;
    int size = 0;
    int clearPos = 0;   // frames already cleared by the current beginClear()
};

} // namespace openmixer::fx
