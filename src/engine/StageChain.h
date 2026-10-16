// Fixed slots of non-owning ProcessStage pointers that can be swapped while audio runs.
#pragma once

#include "ProcessStage.h"

#include <array>
#include <atomic>
#include <cstdint>

namespace openmixer
{

class StageChain
{
public:
    static constexpr int kMaxStages = 4;

    // Control thread. Returns false for a bad slot or an incompatible interface version.
    bool set (int slot, ProcessStage* stage);
    // Control thread. The old stage is safe to destroy once this returns.
    void clear (int slot);

    void prepare (double sampleRate, int maxBlockSize);
    void process (float* left, float* right, int numSamples) noexcept;

private:
    void waitForAudioPass() const;

    std::array<std::atomic<ProcessStage*>, kMaxStages> slots {};
    std::atomic<std::uint32_t> passes { 0 };
    double sampleRate = 44100.0;
    int maxBlock = 512;
};

} // namespace openmixer
