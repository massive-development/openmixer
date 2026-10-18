// Deck transport: play, cue, sync, master, key-lock, quantize, slip. Every button routes a
// "deck.<N>.<id>" parameter; toggles send the opposite of the engine's current state. Message thread only.
#pragma once

#include "DeckBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>

namespace openmixer::ui
{

class TransportPanel : public juce::Component
{
public:
    enum Button { Play, Cue, Sync, Master, KeyLock, Quantize, Slip, NumButtons };

    explicit TransportPanel (DeckBinding binding);

    juce::TextButton& button (Button b) noexcept { return buttons[static_cast<size_t> (b)]; }
    void refresh();   // mirrors engine state onto the toggles
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void press (Button b);
    bool stateOf (Button b) const;
    void applyTheme();

    DeckBinding deckRef;
    std::array<juce::TextButton, NumButtons> buttons;
};

} // namespace openmixer::ui
