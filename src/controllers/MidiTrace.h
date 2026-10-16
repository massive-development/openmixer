// Recorded MIDI stream: {"name":..., "events":[{"time":sec, "bytes":"B0 07 64"}, ...]}.
// Replayed headlessly through a MidiMapper to test mappings without hardware.
#pragma once

#include "MidiMapper.h"

#include <vector>

namespace openmixer
{

struct TimedMidi
{
    double time = 0.0;   // seconds from start
    juce::MidiMessage message;
};

class MidiTrace
{
public:
    juce::String name;
    std::vector<TimedMidi> events;

    void add (double time, const juce::MidiMessage& m) { events.push_back ({ time, m }); }

    static juce::Result fromJson (const juce::String& json, MidiTrace& out);
    static juce::Result fromFile (const juce::File& file, MidiTrace& out);
    juce::String toJson() const;
};

// Replays as fast as possible; returns the total number of parameter updates.
int replayTrace (const MidiTrace& trace, MidiMapper& mapper);

} // namespace openmixer
