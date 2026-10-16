#pragma once

#include "AudioDevice.h"

#include <juce_audio_devices/juce_audio_devices.h>

#include <atomic>

namespace openmixer
{

// Real device via juce::AudioDeviceManager.
class JuceAudioDevice : public AudioDevice, private juce::AudioIODeviceCallback
{
public:
    JuceAudioDevice();
    ~JuceAudioDevice() override;

    // Returns an error string, empty on success.
    juce::String initialise (int numInputChannels = 2, int numOutputChannels = 2,
                             const juce::XmlElement* savedState = nullptr);

    juce::AudioDeviceManager& getDeviceManager() noexcept { return deviceManager; }

    bool start (AudioCallback& callback) override;
    void stop() override;
    bool isRunning() const override { return running.load (std::memory_order_relaxed); }
    juce::String getName() const override;
    double getSampleRate() const override;
    int getBufferSize() const override;

private:
    void audioDeviceIOCallbackWithContext (const float* const*, int, float* const*, int, int,
                                           const juce::AudioIODeviceCallbackContext&) override;
    void audioDeviceAboutToStart (juce::AudioIODevice*) override;
    void audioDeviceStopped() override;

    juce::AudioDeviceManager deviceManager;
    std::atomic<AudioCallback*> userCallback { nullptr };
    std::atomic<bool> running { false };
};

} // namespace openmixer
