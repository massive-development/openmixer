#include "DebugPanel.h"

namespace openmixer::ui
{

namespace { constexpr int kRefreshHz = 30; }

DebugPanel::DebugPanel (DjEngine& e) : engine (e)
{
    for (int i = 0; i < engine.getNumDecks(); ++i)
    {
        decks.push_back (std::make_unique<DeckStrip> (engine.getDeck (i), "Deck " + juce::String (i + 1)));
        channels.push_back (std::make_unique<ChannelStripPanel> (engine.getMixer(), i));
        addAndMakeVisible (*decks.back());
        addAndMakeVisible (*channels.back());
    }
    addMasterControls();
    startTimerHz (kRefreshHz);
}

void DebugPanel::addMasterControls()
{
    auto& mixer = engine.getMixer();
    crossfader.setRange (0.0, 1.0, 0.001);
    crossfader.setValue (0.5, juce::dontSendNotification);
    crossfader.onValueChange = [&mixer, this] { mixer.setParameter ("crossfader", crossfader.getValue()); };

    master.setRange (0.0, 1.5, 0.001);
    master.setValue (1.0, juce::dontSendNotification);
    master.onValueChange = [&mixer, this] { mixer.setParameter ("master", master.getValue()); };

    cueMix.setRange (0.0, 1.0, 0.001);
    cueMix.onValueChange = [&mixer, this] { mixer.setParameter ("cue.mix", cueMix.getValue()); };

    for (juce::Component* c : { static_cast<juce::Component*> (&crossfader), static_cast<juce::Component*> (&master),
                                static_cast<juce::Component*> (&cueMix), static_cast<juce::Component*> (&crossfaderLabel),
                                static_cast<juce::Component*> (&masterLabel), static_cast<juce::Component*> (&cueLabel),
                                static_cast<juce::Component*> (&masterMeterL), static_cast<juce::Component*> (&masterMeterR) })
        addAndMakeVisible (c);
}

void DebugPanel::timerCallback()
{
    for (auto& d : decks)
        d->refresh();
    for (auto& c : channels)
        c->refresh();
    masterMeterL.setLevel (engine.getMixer().getMasterPeakLeft());
    masterMeterR.setLevel (engine.getMixer().getMasterPeakRight());
}

void DebugPanel::paint (juce::Graphics& g)
{
    g.fillAll (juce::Colour (0xFF202028));
}

void DebugPanel::resized()
{
    auto area = getLocalBounds().reduced (8);

    auto masterCol = area.removeFromRight (90);
    masterLabel.setBounds (masterCol.removeFromTop (20));
    auto meters = masterCol.removeFromRight (30);
    masterMeterL.setBounds (meters.removeFromLeft (14));
    masterMeterR.setBounds (meters.removeFromRight (14));
    master.setBounds (masterCol);

    auto bottom = area.removeFromBottom (56);
    crossfaderLabel.setBounds (bottom.removeFromTop (20));
    auto xfRow = bottom.removeFromTop (30);
    crossfader.setBounds (xfRow.removeFromLeft (xfRow.getWidth() * 2 / 3));
    cueLabel.setBounds (xfRow.removeFromTop (14));
    cueMix.setBounds (xfRow);

    const int n = static_cast<int> (decks.size());
    for (int i = 0; i < n; ++i)
    {
        auto column = area.removeFromLeft (area.getWidth() / (n - i));
        decks[static_cast<size_t> (i)]->setBounds (column.removeFromTop (96));
        channels[static_cast<size_t> (i)]->setBounds (column);
    }
}

} // namespace openmixer::ui
