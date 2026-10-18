// What a deck widget needs: read access to one deck's engine state, and a write path that goes
// only through the ParameterRouter ids ("deck.<N>.<id>") so UI and MIDI stay equivalent.
// Message thread only.
#pragma once

#include "../engine/DjEngine.h"

namespace openmixer::ui
{

struct DeckBinding
{
    DjEngine& engine;
    ParameterTarget& target;   // the app's ParameterRouter (tests pass a recorder)
    int index = 0;

    Deck& deck() const noexcept { return engine.getDeck (index); }
    DeckControl& control() const noexcept { return engine.getDeckControl (index); }
    Sync& sync() const noexcept { return engine.getSync(); }

    bool send (const juce::String& id, double value) const
    {
        return target.setParameter ("deck." + juce::String (index) + "." + id, value);
    }
};

} // namespace openmixer::ui
