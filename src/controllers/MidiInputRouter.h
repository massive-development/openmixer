/*
  OpenMixer - routes live juce::MidiInput devices into a MidiMapper.
  Callbacks arrive on JUCE's MIDI thread (not the audio thread). Needs real
  hardware, so it is exercised manually; the mapping logic itself is covered by
  trace-replay tests.
*/
#pragma once

#include "MidiMapper.h"
#include "MidiTrace.h"

#include <juce_audio_devices/juce_audio_devices.h>

namespace openmixer
{

class MidiInputRouter : private juce::MidiInputCallback
{
public:
    MidiInputRouter (juce::AudioDeviceManager& dm, MidiMapper& m) : manager (dm), mapper (m) {}
    ~MidiInputRouter() override { disableAll(); }

    /** Enables every available MIDI input. Returns how many were enabled. */
    int enableAll();
    void disableAll();

    /** While recording, incoming messages are also stored in the trace. */
    void startRecording (MidiTrace* traceToFill) { recording = traceToFill; startTime = juce::Time::getMillisecondCounterHiRes(); }
    void stopRecording() { recording = nullptr; }

private:
    void handleIncomingMidiMessage (juce::MidiInput*, const juce::MidiMessage&) override;

    juce::AudioDeviceManager& manager;
    MidiMapper& mapper;
    juce::StringArray enabled;
    std::atomic<MidiTrace*> recording { nullptr };
    double startTime = 0.0;
};

} // namespace openmixer
