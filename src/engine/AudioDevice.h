// Abstract real-time audio backend and the callback it drives.
#pragma once

#include "LatencyMonitor.h"

#include <juce_core/juce_core.h>

namespace openmixer
{

// process() runs on the audio thread: no allocation, locks or blocking.
class AudioCallback
{
public:
    virtual ~AudioCallback() = default;
    virtual void prepare (double /*sampleRate*/, int /*maxBlockSize*/) {}
    virtual void process (const float* const* inputs, int numInputs,
                          float* const* outputs, int numOutputs, int numSamples) = 0;
    virtual void release() {}
};

class AudioDevice
{
public:
    virtual ~AudioDevice() = default;

    virtual bool start (AudioCallback& callback) = 0;
    // No callback is in flight once this returns.
    virtual void stop() = 0;
    virtual bool isRunning() const = 0;

    virtual juce::String getName() const = 0;
    virtual double getSampleRate() const = 0;
    virtual int getBufferSize() const = 0;

    LatencyMonitor& getLatencyMonitor() noexcept { return monitor; }
    LatencyStats getLatencyStats() const noexcept { return monitor.getStats(); }

protected:
    LatencyMonitor monitor;
};

} // namespace openmixer
