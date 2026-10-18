// Where feedback messages go: a controller's MIDI output port, or a recorder in tests.
#pragma once

#include <juce_audio_basics/juce_audio_basics.h>

#include <mutex>
#include <vector>

namespace openmixer
{

class MidiOutputSink
{
public:
    virtual ~MidiOutputSink() = default;
    virtual void send (const juce::MidiMessage& m) = 0;   // message thread
};

// Test double: keeps every message in order.
class RecordingOutputSink : public MidiOutputSink
{
public:
    void send (const juce::MidiMessage& m) override { sent.push_back (m); }
    std::vector<juce::MidiMessage> sent;
};

} // namespace openmixer
