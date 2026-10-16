#include "DebugPanel.h"

namespace openmixer::ui
{

namespace { constexpr int kRefreshHz = 30, kBottomHeight = 64, kMasterWidth = 90, kDeckHeight = 96; }

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
    setOpaque (true);   // paint() fills everything, so child repaints skip the parent
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

    cueLevel.setRange (0.0, 1.5, 0.001);
    cueLevel.setValue (1.0, juce::dontSendNotification);
    cueLevel.onValueChange = [&mixer, this] { mixer.setParameter ("cue.level", cueLevel.getValue()); };

    styleCaption (crossfaderLabel, "Crossfader");
    styleCaption (masterLabel, "Master");
    styleCaption (cueLabel, "Cue mix");
    styleCaption (cueLevelLabel, "Cue level");
    styleCaption (meterLabelL, "L");
    styleCaption (meterLabelR, "R");

    for (juce::Component* c : { static_cast<juce::Component*> (&crossfader), static_cast<juce::Component*> (&master),
                                static_cast<juce::Component*> (&cueMix), static_cast<juce::Component*> (&cueLevel),
                                static_cast<juce::Component*> (&cueLevelLabel), static_cast<juce::Component*> (&meterLabelL),
                                static_cast<juce::Component*> (&meterLabelR), static_cast<juce::Component*> (&crossfaderLabel),
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
    layoutMaster (area.removeFromRight (kMasterWidth));
    layoutBottom (area.removeFromBottom (kBottomHeight));

    const int n = static_cast<int> (decks.size());
    for (int i = 0; i < n; ++i)
    {
        auto column = area.removeFromLeft (area.getWidth() / (n - i));
        decks[static_cast<size_t> (i)]->setBounds (column.removeFromTop (kDeckHeight));
        channels[static_cast<size_t> (i)]->setBounds (column);
    }
}

void DebugPanel::layoutMaster (juce::Rectangle<int> column)
{
    masterLabel.setBounds (column.removeFromTop (kCaptionHeight));
    auto meters = column.removeFromRight (30);
    auto labels = meters.removeFromTop (kCaptionHeight);
    meterLabelL.setBounds (labels.removeFromLeft (14));
    meterLabelR.setBounds (labels.removeFromRight (14));
    masterMeterL.setBounds (meters.removeFromLeft (14));
    masterMeterR.setBounds (meters.removeFromRight (14));
    master.setBounds (column);
}

void DebugPanel::layoutBottom (juce::Rectangle<int> row)
{
    const int quarter = row.getWidth() / 4;
    placeCaptioned (row.removeFromLeft (row.getWidth() / 2).reduced (2, 0), crossfaderLabel, crossfader);
    placeCaptioned (row.removeFromLeft (quarter).reduced (2, 0), cueLabel, cueMix);
    placeCaptioned (row.reduced (2, 0), cueLevelLabel, cueLevel);
}

} // namespace openmixer::ui
