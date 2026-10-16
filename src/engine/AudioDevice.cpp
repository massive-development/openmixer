#include "AudioDevice.h"

#include <algorithm>

namespace openmixer
{

//==============================================================================
void LatencyMonitor::configure (double sr, int bs) noexcept
{
    sampleRate.store (sr);
    bufferSize.store (bs);
    budgetUs.store (sr > 0.0 ? 1.0e6 * bs / sr : 0.0);
}

void LatencyMonitor::setDeviceLatency (int i, int o) noexcept
{
    inLatency.store (i);
    outLatency.store (o);
}

void LatencyMonitor::reset() noexcept
{
    lastUs.store (0.0);
    totalUs.store (0.0);
    maxUs.store (0.0);
    count.store (0);
    xruns.store (0);
}

void LatencyMonitor::recordCallback (double durationUs, int numSamples) noexcept
{
    // Single writer: plain load/store on atomics is sufficient.
    lastUs.store (durationUs, std::memory_order_relaxed);
    totalUs.store (totalUs.load (std::memory_order_relaxed) + durationUs, std::memory_order_relaxed);
    if (durationUs > maxUs.load (std::memory_order_relaxed))
        maxUs.store (durationUs, std::memory_order_relaxed);

    const double sr = sampleRate.load (std::memory_order_relaxed);
    const double budget = sr > 0.0 ? 1.0e6 * numSamples / sr : budgetUs.load (std::memory_order_relaxed);
    if (budget > 0.0 && durationUs > budget)
        xruns.fetch_add (1, std::memory_order_relaxed);
    count.fetch_add (1, std::memory_order_release);
}

LatencyStats LatencyMonitor::getStats() const noexcept
{
    LatencyStats s;
    s.callbackCount = count.load (std::memory_order_acquire);
    s.sampleRate = sampleRate.load();
    s.bufferSize = bufferSize.load();
    s.inputLatencySamples = inLatency.load();
    s.outputLatencySamples = outLatency.load();
    if (s.sampleRate > 0.0)
    {
        s.inputLatencyMs = 1000.0 * s.inputLatencySamples / s.sampleRate;
        s.outputLatencyMs = 1000.0 * s.outputLatencySamples / s.sampleRate;
        s.roundTripLatencyMs = 1000.0 * (s.bufferSize + s.inputLatencySamples + s.outputLatencySamples) / s.sampleRate;
    }
    s.lastCallbackUs = lastUs.load();
    s.maxCallbackUs = maxUs.load();
    s.avgCallbackUs = s.callbackCount ? totalUs.load() / (double) s.callbackCount : 0.0;
    s.budgetUs = budgetUs.load();
    s.cpuLoad = s.budgetUs > 0.0 ? s.avgCallbackUs / s.budgetUs : 0.0;
    s.xrunCount = xruns.load();
    return s;
}

//==============================================================================
JuceAudioDevice::JuceAudioDevice() = default;

JuceAudioDevice::~JuceAudioDevice()
{
    stop();
    deviceManager.closeAudioDevice();
}

juce::String JuceAudioDevice::initialise (int numIn, int numOut, const juce::XmlElement* state)
{
    return deviceManager.initialise (numIn, numOut, state, true);
}

bool JuceAudioDevice::start (AudioCallback& cb)
{
    if (running.load() || deviceManager.getCurrentAudioDevice() == nullptr)
        return false;

    userCallback.store (&cb);
    running.store (true);
    deviceManager.addAudioCallback (this);   // triggers audioDeviceAboutToStart
    return true;
}

void JuceAudioDevice::stop()
{
    if (! running.exchange (false))
        return;

    // removeAudioCallback synchronises with any in-flight callback.
    deviceManager.removeAudioCallback (this);
    userCallback.store (nullptr);
}

juce::String JuceAudioDevice::getName() const
{
    if (auto* d = deviceManager.getCurrentAudioDevice())
        return d->getName();
    return {};
}

double JuceAudioDevice::getSampleRate() const
{
    if (auto* d = deviceManager.getCurrentAudioDevice())
        return d->getCurrentSampleRate();
    return 0.0;
}

int JuceAudioDevice::getBufferSize() const
{
    if (auto* d = deviceManager.getCurrentAudioDevice())
        return d->getCurrentBufferSizeSamples();
    return 0;
}

void JuceAudioDevice::audioDeviceAboutToStart (juce::AudioIODevice* d)
{
    monitor.reset();
    monitor.configure (d->getCurrentSampleRate(), d->getCurrentBufferSizeSamples());
    monitor.setDeviceLatency (d->getInputLatencyInSamples(), d->getOutputLatencyInSamples());
    if (auto* cb = userCallback.load())
        cb->prepare (d->getCurrentSampleRate(), d->getCurrentBufferSizeSamples());
}

void JuceAudioDevice::audioDeviceStopped()
{
    if (auto* cb = userCallback.load())
        cb->release();
}

void JuceAudioDevice::audioDeviceIOCallbackWithContext (const float* const* in, int numIn,
                                                        float* const* out, int numOut, int numSamples,
                                                        const juce::AudioIODeviceCallbackContext&)
{
    const auto t0 = juce::Time::getHighResolutionTicks();

    if (auto* cb = userCallback.load())
    {
        cb->process (in, numIn, out, numOut, numSamples);
    }
    else
    {
        for (int c = 0; c < numOut; ++c)
            juce::FloatVectorOperations::clear (out[c], numSamples);
    }

    const auto t1 = juce::Time::getHighResolutionTicks();
    monitor.recordCallback (1.0e6 * juce::Time::highResolutionTicksToSeconds (t1 - t0), numSamples);
}

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
                juce::Thread::sleep ((int) wait);
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
    inPtrs.resize ((size_t) numIn);
    outPtrs.resize ((size_t) numOut);
    monitor.configure (sr, bs);
}

MockAudioDevice::~MockAudioDevice() { stop(); }

void MockAudioDevice::setSimulatedLatency (int i, int o) { monitor.setDeviceLatency (i, o); }

bool MockAudioDevice::start (AudioCallback& cb)
{
    if (running.load())
        return false;
    monitor.reset();
    cb.prepare (sampleRate, bufferSize);
    callback.store (&cb);
    running.store (true);
    return true;
}

void MockAudioDevice::stop()
{
    stopRealtime();
    if (! running.exchange (false))
        return;
    if (auto* cb = callback.exchange (nullptr))
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
    auto* cb = callback.load();
    if (cb == nullptr)
        return false;

    for (size_t c = 0; c < inPtrs.size(); ++c)
        inPtrs[c] = inputBuffer.getReadPointer ((int) c);
    for (size_t c = 0; c < outPtrs.size(); ++c)
        outPtrs[c] = outputBuffer.getWritePointer ((int) c);

    const auto t0 = juce::Time::getHighResolutionTicks();
    cb->process (inPtrs.data(), (int) inPtrs.size(), outPtrs.data(), (int) outPtrs.size(), bufferSize);
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
    if (thread == nullptr && running.load())
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
