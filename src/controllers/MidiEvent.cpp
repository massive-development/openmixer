#include "MidiEvent.h"

namespace openmixer
{

bool MidiEvent::fromMessage (const juce::MidiMessage& m, MidiEvent& out) noexcept
{
    if (m.isController())
        out = { Kind::CC, m.getChannel(), m.getControllerNumber(), m.getControllerValue(), false };
    else if (m.isNoteOn())
        out = { Kind::Note, m.getChannel(), m.getNoteNumber(), m.getVelocity(), true };
    else if (m.isNoteOff())
        out = { Kind::Note, m.getChannel(), m.getNoteNumber(), 0, false };
    else if (m.isPitchWheel())
        out = { Kind::PitchBend, m.getChannel(), 0, m.getPitchWheelValue(), false };
    else
        return false;
    return true;
}

} // namespace openmixer
