#include "JuceAudioDevice.h"

namespace openmixer
{

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
    if (running.load (std::memory_order_relaxed) || deviceManager.getCurrentAudioDevice() == nullptr)
        return false;

    userCallback.store (&cb, std::memory_order_release);
    running.store (true, std::memory_order_relaxed);
    deviceManager.addAudioCallback (this);   // triggers audioDeviceAboutToStart
    return true;
}

void JuceAudioDevice::stop()
{
    if (! running.exchange (false, std::memory_order_relaxed))
        return;

    // removeAudioCallback synchronises with any in-flight callback.
    deviceManager.removeAudioCallback (this);
    userCallback.store (nullptr, std::memory_order_release);
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
    if (auto* cb = userCallback.load (std::memory_order_acquire))
        cb->prepare (d->getCurrentSampleRate(), d->getCurrentBufferSizeSamples());
}

void JuceAudioDevice::audioDeviceStopped()
{
    if (auto* cb = userCallback.load (std::memory_order_acquire))
        cb->release();
}

void JuceAudioDevice::audioDeviceIOCallbackWithContext (const float* const* in, int numIn,
                                                        float* const* out, int numOut, int numSamples,
                                                        const juce::AudioIODeviceCallbackContext&)
{
    const auto t0 = juce::Time::getHighResolutionTicks();

    if (auto* cb = userCallback.load (std::memory_order_acquire))
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

} // namespace openmixer
