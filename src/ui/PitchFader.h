// Tempo fader with a selectable range (+-8 / 16 / 50 %). Routes "deck.<N>.rate". Message thread only.
#pragma once

#include "DeckBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class PitchFader : public juce::Component
{
public:
    explicit PitchFader (DeckBinding binding);

    static constexpr double kRanges[] = { 0.08, 0.16, 0.5 };
    void setRangeIndex (int index);   // keeps the current rate when it still fits
    double getRange() const noexcept { return range; }
    juce::Slider& getSlider() noexcept { return fader; }
    juce::ComboBox& getRangeBox() noexcept { return rangeBox; }

    void refresh();   // follows rate changes made elsewhere (sync nudges, MIDI)
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();

    DeckBinding deckRef;
    juce::Slider fader { juce::Slider::LinearVertical, juce::Slider::TextBoxBelow };
    juce::ComboBox rangeBox;
    double range = kRanges[0];
};

} // namespace openmixer::ui
