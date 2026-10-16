/*
  OpenMixer - controller trace harness.

  A trace is a recorded stream of raw MIDI messages stored as JSON:
  { "name": "...", "events": [ { "time": 0.012, "bytes": "B0 07 64" }, ... ] }
  Traces can be recorded from a live device (MidiTrace::add + toJson) and
  replayed headlessly through a MidiMapper to verify a mapping produces the
  expected parameter changes, with no hardware or audio device involved.
*/
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

/** One parameter change observed during replay. */
struct ParameterChange
{
    juce::String id;
    double value = 0.0;
};

/** ParameterTarget that simply records everything it is told. */
class RecordingTarget : public ParameterTarget
{
public:
    std::vector<ParameterChange> changes;

    void setParameter (const juce::String& id, double value) override { changes.push_back ({ id, value }); }

    /** Last recorded value for id, or fallback if never set. */
    double last (const juce::String& id, double fallback = 0.0) const;
    int count (const juce::String& id) const;
};

/** Replays the trace as fast as possible through the mapper.
    Returns the total number of parameter updates produced. */
int replayTrace (const MidiTrace& trace, MidiMapper& mapper);

} // namespace openmixer
