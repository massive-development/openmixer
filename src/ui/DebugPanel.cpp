#include "DebugPanel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kRefreshHz = 30, kHeaderHeight = 28, kDeckHeight = 88;
}

DebugPanel::DebugPanel (DjEngine& e, ParameterTarget& target) : engine (e), mixer (MixerBinding { e, target })
{
    for (int i = 0; i < engine.getNumDecks(); ++i)
    {
        decks.push_back (std::make_unique<DeckStrip> (engine.getDeck (i), "Deck " + juce::String (i + 1), theme::accentFor (i)));
        addAndMakeVisible (*decks.back());
    }
    addAndMakeVisible (mixer);
    setOpaque (true);   // paint() fills everything, so child repaints skip the parent
    startTimerHz (kRefreshHz);
}

void DebugPanel::setBackdropOpacity (float opacity)
{
    backdropOpacity = juce::jlimit (0.0F, 1.0F, opacity);
    setOpaque (backdropOpacity >= 1.0F);
    repaint();
}

void DebugPanel::applyTheme()
{
    for (size_t i = 0; i < decks.size(); ++i)
        decks[i]->setAccent (theme::accentFor (static_cast<int> (i)));
}

void DebugPanel::lookAndFeelChanged() { applyTheme(); }

void DebugPanel::timerCallback()
{
    for (auto& d : decks)
        d->refresh();
}

void DebugPanel::paint (juce::Graphics& g)
{
    if (! isOpaque())
        g.fillAll (juce::Colours::transparentBlack);
    theme::paintBackdrop (g, getLocalBounds().toFloat(), backdropOpacity);

    auto header = headerArea.toFloat();
    g.setColour (theme::textPrimary());
    g.setFont (theme::font (theme::typeHeading(), true));
    g.drawText ("OpenMixer", header, juce::Justification::centredLeft, false);
    g.setColour (theme::textMuted());
    g.setFont (theme::font (theme::typeCaption()));
    g.drawText (juce::String (engine.getNumDecks()) + "-deck debug console", header, juce::Justification::centredRight, false);
}

void DebugPanel::resized()
{
    auto area = getLocalBounds().reduced (theme::gap());
    headerArea = area.removeFromTop (kHeaderHeight);
    area.removeFromTop (theme::gap());

    auto deckRow = area.removeFromTop (kDeckHeight);
    area.removeFromTop (theme::gap());
    const int n = static_cast<int> (decks.size());
    for (int i = 0; i < n; ++i)
    {
        decks[static_cast<size_t> (i)]->setBounds (deckRow.removeFromLeft ((deckRow.getWidth() - (n - 1 - i) * theme::gap()) / (n - i)));
        deckRow.removeFromLeft (theme::gap());
    }
    mixer.setBounds (area);
}

} // namespace openmixer::ui
