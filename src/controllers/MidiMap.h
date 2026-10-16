// User-editable JSON controller map. Example:
//   { "name": "My controller", "mappings": [
//     { "type": "cc", "channel": 1, "number": 7, "target": "mixer.ch0.fader" },
//     { "type": "cc", "number": 21, "target": "mixer.ch0.eq.low", "mode": "relative",
//       "step": 0.5, "min": -48, "max": 6, "initial": 0 },
//     { "type": "note", "number": 36, "target": "deck.0.play", "mode": "toggle" },
//     { "type": "pitchbend", "target": "deck.0.rate", "min": 0.92, "max": 1.08 } ] }
// type: cc|note|pitchbend. channel: 1-16, omitted = any.
// mode: absolute (default), relative (offset 64), relative2c, button (default for notes), toggle.
#pragma once

#include "MidiEvent.h"

#include <juce_core/juce_core.h>

#include <vector>

namespace openmixer
{

enum class MapMode { Absolute, Relative, RelativeTwosComplement, Button, Toggle };

struct Mapping
{
    MidiEvent::Kind kind = MidiEvent::Kind::CC;
    int channel = 0;          // 0 = any
    int number = 0;
    juce::String target;
    MapMode mode = MapMode::Absolute;
    double min = 0.0, max = 1.0, step = 0.01, initial = 0.0;
    bool invert = false;
};

struct MidiMap
{
    juce::String name;
    std::vector<Mapping> mappings;

    // Fails without touching `out` when any mapping is invalid.
    static juce::Result parse (const juce::String& json, MidiMap& out);
};

} // namespace openmixer
