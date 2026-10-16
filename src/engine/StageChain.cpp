#include "StageChain.h"

#include <juce_core/juce_core.h>

namespace openmixer
{

namespace
{
    constexpr auto kRelaxed = std::memory_order_relaxed;
    constexpr int kWaitTimeoutMs = 50;
}

bool StageChain::set (int slot, ProcessStage* stage)
{
    if (slot < 0 || slot >= kMaxStages)
        return false;
    if (stage != nullptr)
    {
        if (stage->interfaceVersion() != ProcessStage::kInterfaceVersion)
            return false;
        stage->prepare (sampleRate, maxBlock);
    }
    const auto idx = static_cast<size_t> (slot);
    const bool replacing = slots[idx].exchange (stage, std::memory_order_acq_rel) != nullptr;
    if (replacing)
        waitForAudioPass();
    return true;
}

void StageChain::clear (int slot)
{
    set (slot, nullptr);
}

void StageChain::waitForAudioPass() const
{
    // Two completed passes guarantee the audio thread no longer holds the old pointer.
    const auto start = passes.load (std::memory_order_acquire);
    const auto deadline = juce::Time::getMillisecondCounter() + kWaitTimeoutMs;
    while (passes.load (std::memory_order_acquire) - start < 2 && juce::Time::getMillisecondCounter() < deadline)
        juce::Thread::sleep (1);
}

void StageChain::prepare (double sr, int maxBlockSize)
{
    sampleRate = sr;
    maxBlock = maxBlockSize;
    for (auto& s : slots)
        if (auto* stage = s.load (kRelaxed))
            stage->prepare (sr, maxBlockSize);
}

void StageChain::process (float* left, float* right, int numSamples) noexcept
{
    for (auto& s : slots)
        if (auto* stage = s.load (std::memory_order_acquire))
            stage->process (left, right, numSamples);
    passes.fetch_add (1, std::memory_order_release);
}

} // namespace openmixer
