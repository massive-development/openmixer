// Applies a MidiMap to incoming events and forwards results to a ParameterTarget.
#pragma once

#include "../core/ParameterTarget.h"
#include "MidiMap.h"

#include <map>
#include <unordered_map>

namespace openmixer
{

class MidiMapper
{
public:
    void setTarget (ParameterTarget* t) noexcept { target = t; }

    juce::Result loadFromJson (const juce::String& json);
    juce::Result loadFromFile (const juce::File& file);

    const juce::String& getName() const noexcept { return map.name; }
    size_t getNumMappings() const noexcept { return map.mappings.size(); }

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

    ParameterTarget* target = nullptr;
    MidiMap map;
    std::unordered_map<int, std::vector<int>> index;
    std::map<juce::String, double> values;
};

} // namespace openmixer
