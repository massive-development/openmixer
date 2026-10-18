// One feedback rule of a controller map: an engine state id drives a note/CC value on the controller (an LED).
// JSON (inside the map's "outputs" array):
//   { "source": "deck.0.play", "type": "note", "channel": 1, "number": 36, "on": 127, "off": 0 }
//   { "source": "deck.0.cue", "type": "note", "number": 37, "blink": "on", "blinkMs": 250 }
//   { "source": "mixer.master.level", "type": "cc", "number": 8, "mode": "level", "off": 0, "on": 127 }
// source: state id read through a StateReader (see EngineStateReader for the list).
// type: note|cc. channel: 1-16 (default 1). mode: bool (default; state >= threshold is "on") | level
//   (state in [min,max] maps linearly onto off..on). on/off: 0-127 (defaults 127/0).
// blink (bool mode): "on" blinks while the state is on, "off" while it is off; blinkMs is the half period (default 250).
#pragma once

#include "MidiEvent.h"

#include <juce_core/juce_core.h>

namespace openmixer
{

struct OutputRule
{
    enum class Mode { Bool, Level };
    enum class Blink { None, WhenOn, WhenOff };

    juce::String source;
    MidiEvent::Kind kind = MidiEvent::Kind::Note;   // Note or CC
    int channel = 1;
    int number = 0;
    Mode mode = Mode::Bool;
    int onValue = 127, offValue = 0;
    double threshold = 0.5;
    double min = 0.0, max = 1.0;
    Blink blink = Blink::None;
    int blinkMs = 250;

    // Fails without touching `out` for an invalid rule.
    static juce::Result parse (const juce::var& v, OutputRule& out);
    juce::var toVar() const;

    // Controller value (0-127) for a state at a time (ms, any monotonic clock).
    int evaluate (double state, double nowMs) const noexcept;
    // A note rule sends note-off for value 0.
    juce::MidiMessage toMessage (int value) const;
};

} // namespace openmixer
