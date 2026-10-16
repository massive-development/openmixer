#include "MockAudioDevice.h"

#include <algorithm>

namespace openmixer
{

//==============================================================================
class MockAudioDevice::PacedThread : public juce::Thread
{
public:
    explicit PacedThread (MockAudioDevice& d) : juce::Thread ("MockAudioDevice"), device (d) {}

    void run() override
    {
        const double blockSeconds = device.bufferSize / device.sampleRate;
        auto next = juce::Time::getMillisecondCounterHiRes();
        while (! threadShouldExit())
        {
            device.renderBlock();
            next += blockSeconds * 1000.0;
            const double wait = next - juce::Time::getMillisecondCounterHiRes();
            if (wait > 0.0)
                juce::Thread::sleep (static_cast<int> (wait));
        }
    }

private:
    MockAudioDevice& device;
};

MockAudioDevice::MockAudioDevice (double sr, int bs, int numIn, int numOut)
    : sampleRate (sr), bufferSize (bs),
      inputBuffer (numIn, bs), outputBuffer (numOut, bs)
{
    inputBuffer.clear();
    outputBuffer.clear();
    inPtrs.resize (static_cast<size_t> (numIn));
    outPtrs.resize (static_cast<size_t> (numOut));
    monitor.configure (sr, bs);
}

MockAudioDevice::~MockAudioDevice() { stop(); }

void MockAudioDevice::setSimulatedLatency (int i, int o) { monitor.setDeviceLatency (i, o); }

bool MockAudioDevice::start (AudioCallback& cb)
{
    if (running.load (std::memory_order_relaxed))
        return false;
    monitor.reset();
    cb.prepare (sampleRate, bufferSize);
    callback.store (&cb, std::memory_order_release);
    running.store (true, std::memory_order_relaxed);
    return true;
}

void MockAudioDevice::stop()
{
    stopRealtime();
    if (! running.exchange (false, std::memory_order_relaxed))
        return;
    if (auto* cb = callback.exchange (nullptr, std::memory_order_acq_rel))
        cb->release();
}

void MockAudioDevice::startCapture (int maxFrames)
{
    capture.setSize (outputBuffer.getNumChannels(), maxFrames);
    capture.clear();
    capturedFrames = 0;
}

bool MockAudioDevice::renderBlock()
{
    auto* cb = callback.load (std::memory_order_acquire);
    if (cb == nullptr)
        return false;

    for (size_t c = 0; c < inPtrs.size(); ++c)
        inPtrs[c] = inputBuffer.getReadPointer (static_cast<int> (c));
    for (size_t c = 0; c < outPtrs.size(); ++c)
        outPtrs[c] = outputBuffer.getWritePointer (static_cast<int> (c));

    const auto t0 = juce::Time::getHighResolutionTicks();
    cb->process (inPtrs.data(), static_cast<int> (inPtrs.size()), outPtrs.data(), static_cast<int> (outPtrs.size()), bufferSize);
    const auto t1 = juce::Time::getHighResolutionTicks();
    monitor.recordCallback (1.0e6 * juce::Time::highResolutionTicksToSeconds (t1 - t0), bufferSize);

    if (capture.getNumSamples() > 0)
    {
        const int n = std::min (bufferSize, capture.getNumSamples() - capturedFrames);
        for (int c = 0; c < capture.getNumChannels(); ++c)
            capture.copyFrom (c, capturedFrames, outputBuffer, c, 0, n);
        capturedFrames += n;
    }
    return true;
}

int MockAudioDevice::renderBlocks (int numBlocks)
{
    int done = 0;
    while (done < numBlocks && renderBlock())
        ++done;
    return done;
}

void MockAudioDevice::startRealtime()
{
    if (thread == nullptr && running.load (std::memory_order_relaxed))
    {
        thread = std::make_unique<PacedThread> (*this);
        thread->startThread();
    }
}

void MockAudioDevice::stopRealtime()
{
    if (thread != nullptr)
    {
        thread->stopThread (2000);
        thread.reset();
    }
}

} // namespace openmixer
