// Eight hotcue pads. Click sets an empty cue at the playhead or jumps to a set one; Shift/Alt/Ctrl+click
// (or pressPad with clear) deletes it. Routes "deck.<N>.hotcue<i>" and "...hotcue<i>.clear". Message thread only.
#pragma once

#include "DeckBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>

namespace openmixer::ui
{

class HotcuePads : public juce::Component
{
public:
    explicit HotcuePads (DeckBinding binding);

    void pressPad (int index, bool clear);
    juce::TextButton& pad (int index) noexcept { return pads[static_cast<size_t> (index)]; }
    void refresh();   // lights pads whose cue is set
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();

    DeckBinding deckRef;
    std::array<juce::TextButton, HotcueBank::kNumCues> pads;
};

} // namespace openmixer::ui
