// Loop bounds shared from control threads to the decoder and audio threads.
// Writers serialise on a mutex (control side only); readers take a lock-free seqlock snapshot.
#pragma once

#include <atomic>
#include <cstdint>
#include <mutex>

namespace openmixer
{

struct LoopSnapshot
{
    std::int64_t start = 0, end = 0;
    bool enabled = false;
    bool valid() const noexcept { return end > start; }
    bool active() const noexcept { return enabled && valid(); }
};

class LoopState
{
public:
    void store (const LoopSnapshot& s) noexcept
    {
        const std::lock_guard<std::mutex> lock (writeMutex);
        const auto v = seq.load (std::memory_order_relaxed);
        seq.store (v + 1, std::memory_order_relaxed);
        // Release stores (not a fence) keep the odd seq visible before any new field; TSan understands them.
        start.store (s.start, std::memory_order_release);
        end.store (s.end, std::memory_order_release);
        enabled.store (s.enabled, std::memory_order_release);
        seq.store (v + 2, std::memory_order_release);
    }

    // False only while a writer is mid-update; callers keep their previous snapshot.
    bool load (LoopSnapshot& out) const noexcept
    {
        for (int attempt = 0; attempt < 4; ++attempt)
        {
            const auto v = seq.load (std::memory_order_acquire);
            if ((v & 1U) != 0)
                continue;
            const LoopSnapshot s { start.load (std::memory_order_acquire), end.load (std::memory_order_acquire),
                                   enabled.load (std::memory_order_acquire) };
            if (seq.load (std::memory_order_relaxed) == v)
            {
                out = s;
                return true;
            }
        }
        return false;
    }

private:
    std::mutex writeMutex;
    std::atomic<std::uint32_t> seq { 0 };
    std::atomic<std::int64_t> start { 0 }, end { 0 };
    std::atomic<bool> enabled { false };
};

} // namespace openmixer
