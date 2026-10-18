// What the mixer widgets need: read access to the engine's mixer/FX state and a write path that
// goes only through ParameterRouter ids ("mixer.<id>", "fx.<scope>.<id>"), so UI and MIDI stay
// equivalent. Message thread only.
#pragma once

#include "../engine/DjEngine.h"

namespace openmixer::ui
{

struct MixerBinding
{
    DjEngine& engine;
    ParameterTarget& target;   // the app's ParameterRouter (tests pass a recorder)

    Mixer& mixer() const noexcept { return engine.getMixer(); }
    FxRouting& fx() const noexcept { return engine.getFx(); }
    int numChannels() const noexcept { return mixer().getNumChannels(); }

    bool send (const juce::String& id, double value) const { return target.setParameter ("mixer." + id, value); }
    bool sendFx (const juce::String& id, double value) const { return target.setParameter ("fx." + id, value); }

    static juce::String channelId (int ch, const juce::String& id) { return "ch" + juce::String (ch) + "." + id; }
};

} // namespace openmixer::ui
