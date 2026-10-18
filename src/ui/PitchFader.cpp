#include "PitchFader.h"

#include "Theme.h"
#include "ValueFormat.h"

#include <algorithm>

namespace openmixer::ui
{

namespace
{
    constexpr int kTextWidth = 64, kTextHeight = 20;
    constexpr double kStep = 0.0005;
}

PitchFader::PitchFader (DeckBinding binding) : deckRef (binding)
{
    fader.textFromValueFunction = format::rate;   // before setValue, which renders the text
    fader.setTextBoxStyle (juce::Slider::TextBoxBelow, true, kTextWidth, kTextHeight);
    fader.setDoubleClickReturnValue (true, 1.0);
    fader.onValueChange = [this] { deckRef.send ("rate", fader.getValue()); };
    for (size_t i = 0; i < std::size (kRanges); ++i)
        rangeBox.addItem (juce::String::fromUTF8 ("\xc2\xb1") + juce::String (juce::roundToInt (kRanges[i] * 100.0)) + "%", static_cast<int> (i) + 1);
    rangeBox.onChange = [this] { setRangeIndex (rangeBox.getSelectedId() - 1); };
    addAndMakeVisible (fader);
    addAndMakeVisible (rangeBox);
    rangeBox.setSelectedId (1, juce::dontSendNotification);
    setRangeIndex (0);
    applyTheme();
}

void PitchFader::applyTheme() { fader.setColour (juce::Slider::trackColourId, theme::accentFor (deckRef.index)); }

void PitchFader::setRangeIndex (int index)
{
    range = kRanges[std::clamp (index, 0, static_cast<int> (std::size (kRanges)) - 1)];
    rangeBox.setSelectedId (static_cast<int> (std::distance (std::begin (kRanges), std::find (std::begin (kRanges), std::end (kRanges), range))) + 1,
                            juce::dontSendNotification);
    fader.setRange (1.0 - range, 1.0 + range, kStep);
    const double engineRate = deckRef.deck().getRate(), fitted = std::clamp (engineRate, 1.0 - range, 1.0 + range);
    fader.setValue (fitted, juce::dontSendNotification);
    if (fitted != engineRate)
        deckRef.send ("rate", fitted);   // a narrower range pulls the tempo back inside it
}

void PitchFader::refresh()
{
    const double rate = deckRef.deck().getRate();
    if (! fader.isMouseButtonDown() && std::abs (fader.getValue() - rate) > kStep)
        fader.setValue (rate, juce::dontSendNotification);
}

void PitchFader::resized()
{
    auto area = getLocalBounds();
    rangeBox.setBounds (area.removeFromTop (theme::deckRangeBoxHeight()));
    area.removeFromTop (theme::innerGap());
    fader.setBounds (area);
}

} // namespace openmixer::ui
