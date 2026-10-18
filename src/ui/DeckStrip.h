// One deck as a glass card: title + position readout, load / play, rate, and a progress line.
// Message thread only.
#pragma once

#include "../engine/Deck.h"
#include "ControlCaption.h"
#include "Theme.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>

namespace openmixer::ui
{

class DeckStrip : public juce::Component
{
public:
    DeckStrip (Deck& deck, const juce::String& title, juce::Colour accent = theme::accentFor (0));

    static juce::String formatPosition (double seconds, double total);

    void refresh();
    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override;
    void setAccent (juce::Colour newAccent);   // DebugPanel re-resolves per-deck accents on theme switch

private:
    void applyTheme();
    void chooseFile();
    juce::Rectangle<float> progressTrack() const;

    Deck& deck;
    juce::Colour accent;
    juce::Label titleLabel, positionLabel, rateCaption;
    juce::TextButton loadButton { "Load" }, playButton { "Play" };
    juce::Slider rateSlider { juce::Slider::LinearHorizontal, juce::Slider::TextBoxRight };
    std::unique_ptr<juce::FileChooser> chooser;
    bool choosing = false;
    float progress = 0.0F;
};

} // namespace openmixer::ui
