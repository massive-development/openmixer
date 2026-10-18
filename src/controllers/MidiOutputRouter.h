// Opens the MIDI output ports that belong to the enabled inputs (same device name) and fans feedback out to them.
// Needs hardware, so only the pairing rule (pairOutputs) is unit tested. Message thread only.
#pragma once

#include "MidiOutputSink.h"

#include <juce_audio_devices/juce_audio_devices.h>

#include <memory>

namespace openmixer
{

class MidiOutputRouter : public MidiOutputSink
{
public:
    // Output devices whose name matches an enabled input's name (or every output when `all`).
    static juce::Array<juce::MidiDeviceInfo> pairOutputs (const juce::Array<juce::MidiDeviceInfo>& inputs,
                                                          const juce::StringArray& enabledInputIds, bool all,
                                                          const juce::Array<juce::MidiDeviceInfo>& outputs);

    // Replaces the open ports with the ones paired to these inputs. Returns how many opened.
    int openPairedWith (const juce::StringArray& enabledInputIds, bool all);
    void closeAll() { ports.clear(); }
    int getNumOpen() const noexcept { return static_cast<int> (ports.size()); }

    void send (const juce::MidiMessage& m) override;

private:
    std::vector<std::unique_ptr<juce::MidiOutput>> ports;
};

} // namespace openmixer
