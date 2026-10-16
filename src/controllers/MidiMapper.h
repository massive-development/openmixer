/*
  OpenMixer - MIDI mapping engine.

  Translates incoming MIDI (CC, note, pitch bend) into named engine parameters
  using a JSON map. The mapper is decoupled from the engine through the
  ParameterTarget interface, so mappings can be tested headlessly with a
  recording target (see MidiTrace.h) and bound to real engine objects with
  EngineTarget.

  Map format (all fields except "target" and "type" optional):
  {
    "name": "My controller",
    "mappings": [
      { "type": "cc", "channel": 1, "number": 7, "target": "mixer.ch0.fader",
        "mode": "absolute", "min": 0.0, "max": 1.0, "invert": false },
      { "type": "cc", "number": 21, "target": "mixer.ch0.eq.low",
        "mode": "relative", "step": 0.5, "min": -48, "max": 6, "initial": 0 },
      { "type": "note", "channel": 1, "number": 36, "target": "deck.0.play", "mode": "toggle" },
      { "type": "pitchbend", "target": "deck.0.rate", "min": 0.92, "max": 1.08 }
    ]
  }
  type: cc | note | pitchbend.   channel: 1-16 (omitted = any).
  mode: absolute (default for cc/pitchbend) | relative (offset-64) | relative2c
        (two's complement) | button (press = max, release = min) | toggle.
  Notes default to button mode.
*/
#pragma once

#include <juce_core/juce_core.h>
#include <juce_audio_basics/juce_audio_basics.h>

#include <map>
#include <unordered_map>
#include <vector>

namespace openmixer
{

struct MidiEvent
{
    enum class Kind { CC, Note, PitchBend };
    Kind kind = Kind::CC;
    int channel = 1;      // 1-16
    int number = 0;       // controller / note number (0 for pitch bend)
    int value = 0;        // 0-127, or 0-16383 for pitch bend; note-off has value 0
    bool noteOn = false;

    /** Returns false for message types the mapper does not handle. */
    static bool fromMessage (const juce::MidiMessage& m, MidiEvent& out) noexcept;
};

class ParameterTarget
{
public:
    virtual ~ParameterTarget() = default;
    virtual void setParameter (const juce::String& id, double value) = 0;
};

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

class MidiMapper
{
public:
    void setTarget (ParameterTarget* t) noexcept { target = t; }

    juce::Result loadFromJson (const juce::String& json);
    juce::Result loadFromFile (const juce::File& file);

    const juce::String& getName() const noexcept { return name; }
    size_t getNumMappings() const noexcept { return mappings.size(); }
    const std::vector<Mapping>& getMappings() const noexcept { return mappings; }

    /** Feed one MIDI message. Returns the number of parameter updates produced. */
    int handleMessage (const juce::MidiMessage& m);
    int handleEvent (const MidiEvent& e);

    /** Last value this mapper produced for a target (or its initial value). */
    double getValue (const juce::String& targetId) const;

private:
    static int makeKey (MidiEvent::Kind k, int channel, int number) noexcept
    {
        return ((int) k << 16) | (channel << 8) | number;
    }

    void apply (const Mapping& m, const MidiEvent& e, int& fired);

    ParameterTarget* target = nullptr;
    juce::String name;
    std::vector<Mapping> mappings;
    std::unordered_map<int, std::vector<int>> index;
    std::map<juce::String, double> values;
};

} // namespace openmixer
