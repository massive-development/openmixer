#include "LoopPrefetch.h"

namespace openmixer
{

LoopPrefetch::LoopPrefetch (std::int64_t framesPerSlot) : capacity (framesPerSlot)
{
    for (auto& s : slots)
        s.frames.assign (static_cast<size_t> (framesPerSlot) * 2, 0.0F);
}

LoopPrefetch::Slot& LoopPrefetch::beginFill() noexcept
{
    published.store (-1);
    return slots[inUse.load() == 0 ? 1 : 0];
}

void LoopPrefetch::publish (const Slot& slot) noexcept
{
    published.store (&slot == &slots[0] ? 0 : 1);
}

const LoopPrefetch::Slot* LoopPrefetch::acquire() noexcept
{
    const int idx = published.load();
    if (idx < 0)
    {
        inUse.store (-1);
        return nullptr;
    }
    inUse.store (idx);
    if (published.load() != idx)   // decoder withdrew it between our two loads
    {
        inUse.store (-1);
        return nullptr;
    }
    return &slots[static_cast<size_t> (idx)];
}

} // namespace openmixer
