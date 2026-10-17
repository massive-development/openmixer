// Pre-decoded audio from a loop start, so the audio thread can jump there without waiting on the decoder.
// Decoder thread fills/publishes; the audio thread acquires/releases (hazard-slot protocol, seq_cst).
#pragma once

#include <array>
#include <atomic>
#include <cstdint>
#include <vector>

namespace openmixer
{

class LoopPrefetch
{
public:
    struct Slot
    {
        std::vector<float> frames;   // interleaved L,R
        std::int64_t start = 0, length = 0;
        std::uint32_t loadId = 0;
        bool contains (std::int64_t frame) const noexcept { return frame >= start && frame < start + length; }
    };

    explicit LoopPrefetch (std::int64_t framesPerSlot);

    std::int64_t getCapacityFrames() const noexcept { return capacity; }
    bool isPublished() const noexcept { return published.load() >= 0; }

    // Decoder thread: unpublishes, then returns a slot the audio thread is not reading.
    Slot& beginFill() noexcept;
    void publish (const Slot& slot) noexcept;
    void withdraw() noexcept { published.store (-1); }

    // Audio thread: the published slot, held until release() or the next acquire().
    const Slot* acquire() noexcept;
    void release() noexcept { inUse.store (-1); }

private:
    std::int64_t capacity;
    std::array<Slot, 2> slots;
    std::atomic<int> published { -1 }, inUse { -1 };
};

} // namespace openmixer
