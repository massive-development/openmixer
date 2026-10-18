// Routes live MIDI inputs into a MidiMapper on JUCE's MIDI thread; needs hardware, so tested via traces.
#pragma once

#include "MidiMapper.h"
#include "MidiTrace.h"

#include <juce_audio_devices/juce_audio_devices.h>

#include <atomic>

namespace openmixer
{

class MidiInputRouter : private juce::MidiInputCallback
{
public:
    MidiInputRouter (juce::AudioDeviceManager& dm, MidiMapper& m) : manager (dm), mapper (m) {}
    ~MidiInputRouter() override { disableAll(); }

    /** Enables every available MIDI input. Returns how many were enabled. */
    int enableAll();
    /** Enables only the inputs with these identifiers (all of them when `all`). Call disableAll() first to replace a selection. */
    int enable (const juce::StringArray& identifiers, bool all = false);
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
