// User-editable JSON controller map. Example:
//   { "name": "My controller", "mappings": [
//     { "type": "cc", "channel": 1, "number": 7, "target": "mixer.ch0.fader" },
//     { "type": "cc", "number": 21, "target": "mixer.ch0.eq.low", "mode": "relative",
//       "step": 0.5, "min": -48, "max": 6, "initial": 0 },
//     { "type": "note", "number": 36, "target": "deck.0.play", "mode": "toggle" },
//     { "type": "pitchbend", "target": "deck.0.rate", "min": 0.92, "max": 1.08 } ] }
// type: cc|note|pitchbend. channel: 1-16, omitted = any.
// mode: absolute (default), relative (offset 64), relative2c, button (default for notes), toggle,
//   absolute14 (CC pair: "number" is the MSB, "lsb" the LSB controller, default number + 32).
// Schema versions: the root's "version" is 2 (current); a file without it is version 1 and loads unchanged.
//   v2 adds the "absolute14" mode and the "outputs" array of feedback rules (see MidiOutputRule.h).
//   A version newer than kVersion is rejected.
#pragma once

#include "MidiEvent.h"
#include "MidiOutputRule.h"

#include <juce_core/juce_core.h>

#include <vector>

namespace openmixer
{

enum class MapMode { Absolute, Relative, RelativeTwosComplement, Button, Toggle, Absolute14 };

juce::String toString (MapMode mode);

struct Mapping
{
    MidiEvent::Kind kind = MidiEvent::Kind::CC;
    int channel = 0;          // 0 = any
    int number = 0;
    juce::String target;
    MapMode mode = MapMode::Absolute;
    double min = 0.0, max = 1.0, step = 0.01, initial = 0.0;
    bool invert = false;
    int lsbNumber = -1;       // Absolute14 only: the LSB controller number

    bool operator== (const Mapping& o) const noexcept;
    juce::Result validate() const;   // the same rules parse applies
};

struct MidiMap
{
    static constexpr int kVersion = 2;

    int version = kVersion;
    juce::String name;
    std::vector<Mapping> mappings;
    std::vector<OutputRule> outputs;

    // Fails without touching `out` when any mapping is invalid.
    static juce::Result parse (const juce::String& json, MidiMap& out);
    // Always writes the current schema version; parse (toJson()) round-trips.
    juce::String toJson() const;
};

} // namespace openmixer
