#include "MidiInputRouter.h"

namespace openmixer
{

int MidiInputRouter::enableAll()
{
    int n = 0;
    for (auto& d : juce::MidiInput::getAvailableDevices())
    {
        manager.setMidiInputDeviceEnabled (d.identifier, true);
        manager.addMidiInputDeviceCallback (d.identifier, this);
        enabled.add (d.identifier);
        ++n;
    }
    return n;
}

void MidiInputRouter::disableAll()
{
    for (auto& id : enabled)
        manager.removeMidiInputDeviceCallback (id, this);
    enabled.clear();
}

void MidiInputRouter::handleIncomingMidiMessage (juce::MidiInput*, const juce::MidiMessage& m)
{
    if (auto* t = recording.load())
        t->add ((juce::Time::getMillisecondCounterHiRes() - startTime) / 1000.0, m);   // debug use only: allocates
    mapper.handleMessage (m);
}

} // namespace openmixer
