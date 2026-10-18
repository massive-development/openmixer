// Applies a MidiMap to incoming events and forwards results to a ParameterTarget.
#pragma once

#include "../core/ParameterTarget.h"
#include "MidiMap.h"

#include <map>
#include <mutex>
#include <unordered_map>

namespace openmixer
{

class MidiMapper
{
public:
    // Thread-safe between control threads: a map reload (install) is atomic with respect to handleMessage on the
    // MIDI thread. The lock is held across the target call; never call from the audio thread.
    void setTarget (ParameterTarget* t) noexcept { const std::lock_guard<std::recursive_mutex> guard (lock); target = t; }

    juce::Result loadFromJson (const juce::String& json);
    juce::Result loadFromFile (const juce::File& file);

    juce::String getName() const { const std::lock_guard<std::recursive_mutex> guard (lock); return map.name; }
    size_t getNumMappings() const { const std::lock_guard<std::recursive_mutex> guard (lock); return map.mappings.size(); }

    // Returns the number of parameter updates produced.
    int handleMessage (const juce::MidiMessage& m);
    int handleEvent (const MidiEvent& e);

    // Last value produced for a target, or its initial value.
    double getValue (const juce::String& targetId) const;

private:
    static int makeKey (MidiEvent::Kind k, int channel, int number) noexcept
    {
        return (static_cast<int> (k) << 16) | (channel << 8) | number;
    }

    void install (MidiMap&& newMap);
    double compute (const Mapping& m, const MidiEvent& e, double current, bool& changed) const noexcept;
    bool apply (const Mapping& m, const MidiEvent& e);

    mutable std::recursive_mutex lock;
    ParameterTarget* target = nullptr;
    MidiMap map;
    std::unordered_map<int, std::vector<int>> index;
    std::map<juce::String, double> values;
};

} // namespace openmixer
