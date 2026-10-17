// Hotcue positions for one deck; set/cleared from control threads, readable from any thread.
#pragma once

#include <algorithm>
#include <array>
#include <atomic>
#include <cstdint>
#include <optional>

namespace openmixer
{

class HotcueBank
{
public:
    static constexpr int kNumCues = 8;

    HotcueBank() noexcept { clearAll(); }

    void set (int index, std::int64_t frame) noexcept { if (inRange (index)) at (index).store (std::max<std::int64_t> (frame, 0)); }
    void clear (int index) noexcept                   { if (inRange (index)) at (index).store (kEmpty); }
    void clearAll() noexcept                          { for (auto& c : cues) c.store (kEmpty); }

    std::optional<std::int64_t> get (int index) const noexcept
    {
        if (! inRange (index))
            return std::nullopt;
        const auto v = cues[static_cast<size_t> (index)].load();
        return v == kEmpty ? std::nullopt : std::optional<std::int64_t> (v);
    }

    static constexpr bool inRange (int index) noexcept { return index >= 0 && index < kNumCues; }

private:
    static constexpr std::int64_t kEmpty = -1;
    std::atomic<std::int64_t>& at (int index) noexcept { return cues[static_cast<size_t> (index)]; }

    std::array<std::atomic<std::int64_t>, kNumCues> cues;
};

} // namespace openmixer
