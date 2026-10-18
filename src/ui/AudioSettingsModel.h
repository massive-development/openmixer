// What the audio settings page needs from the audio layer. The app implements it over the live
// AudioDeviceManager (JuceAudioSettingsModel); tests use a fake, so no real device is ever opened.
// Setters return an error message, empty on success. Message thread only.
#pragma once

#include <juce_core/juce_core.h>

namespace openmixer::ui
{

class AudioSettingsModel
{
public:
    virtual ~AudioSettingsModel() = default;

    virtual juce::StringArray getDeviceTypes() const = 0;
    virtual juce::String getDeviceType() const = 0;
    virtual juce::String setDeviceType (const juce::String& type) = 0;

    virtual juce::StringArray getOutputDevices() const = 0;
    virtual juce::String getOutputDevice() const = 0;
    virtual juce::String setOutputDevice (const juce::String& name) = 0;

    virtual juce::Array<double> getSampleRates() const = 0;
    virtual double getSampleRate() const = 0;
    virtual juce::String setSampleRate (double rate) = 0;

    virtual juce::Array<int> getBufferSizes() const = 0;
    virtual int getBufferSize() const = 0;
    virtual juce::String setBufferSize (int samples) = 0;

    virtual int getNumOutputChannels() const = 0;   // of the open device; 0 when none is open
    virtual juce::String getStateXml() const = 0;   // restorable by the app's start-up code
};

} // namespace openmixer::ui
