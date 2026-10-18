#include "JuceAudioSettingsModel.h"

namespace openmixer::ui
{

juce::StringArray JuceAudioSettingsModel::getDeviceTypes() const
{
    juce::StringArray names;
    for (auto* type : dm.getAvailableDeviceTypes())
        names.add (type->getTypeName());
    return names;
}

juce::String JuceAudioSettingsModel::getDeviceType() const { return dm.getCurrentAudioDeviceType(); }

juce::String JuceAudioSettingsModel::setDeviceType (const juce::String& type)
{
    dm.setCurrentAudioDeviceType (type, true);
    return {};
}

juce::StringArray JuceAudioSettingsModel::getOutputDevices() const
{
    if (auto* type = dm.getCurrentDeviceTypeObject())
        return type->getDeviceNames (false);
    return {};
}

juce::String JuceAudioSettingsModel::getOutputDevice() const
{
    return dm.getAudioDeviceSetup().outputDeviceName;
}

// All outputs on: the pair choice happens in OutputRouter, not by opening fewer channels.
juce::String JuceAudioSettingsModel::applySetup (juce::AudioDeviceManager::AudioDeviceSetup setup)
{
    setup.useDefaultOutputChannels = false;
    setup.outputChannels.clear();
    setup.outputChannels.setRange (0, kMaxOutputs, true);
    return dm.setAudioDeviceSetup (setup, true);
}

juce::String JuceAudioSettingsModel::setOutputDevice (const juce::String& name)
{
    auto setup = dm.getAudioDeviceSetup();
    setup.outputDeviceName = name;
    return applySetup (setup);
}

juce::Array<double> JuceAudioSettingsModel::getSampleRates() const
{
    if (auto* device = dm.getCurrentAudioDevice())
        return device->getAvailableSampleRates();
    return {};
}

double JuceAudioSettingsModel::getSampleRate() const { return dm.getAudioDeviceSetup().sampleRate; }

juce::String JuceAudioSettingsModel::setSampleRate (double rate)
{
    auto setup = dm.getAudioDeviceSetup();
    setup.sampleRate = rate;
    return applySetup (setup);
}

juce::Array<int> JuceAudioSettingsModel::getBufferSizes() const
{
    if (auto* device = dm.getCurrentAudioDevice())
        return device->getAvailableBufferSizes();
    return {};
}

int JuceAudioSettingsModel::getBufferSize() const { return dm.getAudioDeviceSetup().bufferSize; }

juce::String JuceAudioSettingsModel::setBufferSize (int samples)
{
    auto setup = dm.getAudioDeviceSetup();
    setup.bufferSize = samples;
    return applySetup (setup);
}

int JuceAudioSettingsModel::getNumOutputChannels() const
{
    if (auto* device = dm.getCurrentAudioDevice())
        return device->getActiveOutputChannels().countNumberOfSetBits();
    return 0;
}

juce::String JuceAudioSettingsModel::getStateXml() const
{
    const auto xml = dm.createStateXml();
    return xml != nullptr ? xml->toString (juce::XmlElement::TextFormat().singleLine()) : juce::String();
}

} // namespace openmixer::ui
