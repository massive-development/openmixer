#include "MidiOutputRouter.h"

namespace openmixer
{

juce::Array<juce::MidiDeviceInfo> MidiOutputRouter::pairOutputs (const juce::Array<juce::MidiDeviceInfo>& inputs,
                                                                 const juce::StringArray& enabledIds, bool all,
                                                                 const juce::Array<juce::MidiDeviceInfo>& outputs)
{
    juce::StringArray names;
    for (const auto& in : inputs)
        if (all || enabledIds.contains (in.identifier))
            names.add (in.name);

    juce::Array<juce::MidiDeviceInfo> paired;
    for (const auto& out : outputs)
        if (names.contains (out.name))
            paired.add (out);
    return paired;
}

int MidiOutputRouter::openPairedWith (const juce::StringArray& enabledIds, bool all)
{
    closeAll();
    for (const auto& d : pairOutputs (juce::MidiInput::getAvailableDevices(), enabledIds, all, juce::MidiOutput::getAvailableDevices()))
        if (auto port = juce::MidiOutput::openDevice (d.identifier))
            ports.push_back (std::move (port));
    return getNumOpen();
}

void MidiOutputRouter::send (const juce::MidiMessage& m)
{
    for (auto& p : ports)
        p->sendMessageNow (m);
}

} // namespace openmixer
