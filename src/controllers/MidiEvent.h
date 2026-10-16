// Normalised MIDI event (CC, note, pitch bend) extracted from a raw message.
#pragma once

#include <juce_audio_basics/juce_audio_basics.h>

namespace openmixer
{

struct MidiEvent
{
    enum class Kind { CC, Note, PitchBend };

    Kind kind = Kind::CC;
    int channel = 1;      // 1-16
    int number = 0;       // controller or note number; 0 for pitch bend
    int value = 0;        // 0-127, or 0-16383 for pitch bend; note-off is 0
    bool noteOn = false;

    // Returns false for message types the mapper does not handle.
    static bool fromMessage (const juce::MidiMessage& m, MidiEvent& out) noexcept;
};

} // namespace openmixer
