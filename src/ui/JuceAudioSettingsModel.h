// AudioSettingsModel over the app's juce::AudioDeviceManager. Every output channel the device has (up
// to kMaxOutputs) is opened so OutputRouter can send the master and cue mixes to any stereo pair.
#pragma once

#include "AudioSettingsModel.h"

#include <juce_audio_devices/juce_audio_devices.h>

namespace openmixer::ui
{

class JuceAudioSettingsModel : public AudioSettingsModel
{
public:
    static constexpr int kMaxOutputs = 8;

    explicit JuceAudioSettingsModel (juce::AudioDeviceManager& manager) : dm (manager) {}

    juce::StringArray getDeviceTypes() const override;
    juce::String getDeviceType() const override;
    juce::String setDeviceType (const juce::String& type) override;
    juce::StringArray getOutputDevices() const override;
    juce::String getOutputDevice() const override;
    juce::String setOutputDevice (const juce::String& name) override;
    juce::Array<double> getSampleRates() const override;
    double getSampleRate() const override;
    juce::String setSampleRate (double rate) override;
    juce::Array<int> getBufferSizes() const override;
    int getBufferSize() const override;
    juce::String setBufferSize (int samples) override;
    int getNumOutputChannels() const override;
    juce::String getStateXml() const override;

private:
    juce::String applySetup (juce::AudioDeviceManager::AudioDeviceSetup setup);

    juce::AudioDeviceManager& dm;
};

} // namespace openmixer::ui
