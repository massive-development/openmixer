// Debug controls for one deck: load, play/pause, rate, position.
#pragma once

#include "../engine/Deck.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>

namespace openmixer::ui
{

class DeckStrip : public juce::Component
{
public:
    DeckStrip (Deck& deck, const juce::String& title);

    void refresh();
    void resized() override;

private:
    void chooseFile();

    Deck& deck;
    juce::Label titleLabel, positionLabel;
    juce::TextButton loadButton { "Load" }, playButton { "Play" };
    juce::Slider rateSlider { juce::Slider::LinearHorizontal, juce::Slider::TextBoxRight };
    std::unique_ptr<juce::FileChooser> chooser;
};

} // namespace openmixer::ui
