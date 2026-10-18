// Maps the engine's fixed outputs (0/1 master, 2/3 headphone cue) onto chosen stereo pairs of the device.
// Pair p = device channels 2p and 2p+1. Setters are control-thread atomics; process() is audio-thread safe
// (no allocation, locks or blocking) after prepare().
#pragma once

#include "AudioDevice.h"

#include <juce_audio_basics/juce_audio_basics.h>

#include <atomic>

namespace openmixer
{

class OutputRouter
{
public:
    static constexpr int kNoPair = -1;

    void prepare (int maxBlockSize);
    void setMasterPair (int pair) noexcept { masterPair.store (pair, std::memory_order_relaxed); }
    void setCuePair (int pair) noexcept { cuePair.store (pair, std::memory_order_relaxed); }
    int getMasterPair() const noexcept { return masterPair.load (std::memory_order_relaxed); }
    int getCuePair() const noexcept { return cuePair.load (std::memory_order_relaxed); }

    // Renders `source` in chunks and writes the master/cue pairs; unrouted device channels are silenced.
    void process (AudioCallback& source, float* const* deviceOut, int numDeviceOut, int numSamples);

private:
    void route (float* const* deviceOut, int numDeviceOut, int offset, int n) noexcept;

    juce::AudioBuffer<float> scratch { 4, 0 };
    std::atomic<int> masterPair { 0 }, cuePair { 1 };
};

} // namespace openmixer
