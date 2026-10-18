#include "DebugPanel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kRefreshHz = 30, kHeaderHeight = 28, kDeckHeight = 88, kBottomHeight = 76, kMasterWidth = 132;
    constexpr int kMasterMeterWidth = 12, kMaxFaderWidth = 64;
}

DebugPanel::DebugPanel (DjEngine& e) : engine (e)
{
    for (int i = 0; i < engine.getNumDecks(); ++i)
    {
        decks.push_back (std::make_unique<DeckStrip> (engine.getDeck (i), "Deck " + juce::String (i + 1), theme::accentFor (i)));
        channels.push_back (std::make_unique<ChannelStripPanel> (engine.getMixer(), i));
        addAndMakeVisible (*decks.back());
        addAndMakeVisible (*channels.back());
    }
    addMasterControls();
    setOpaque (true);   // paint() fills everything, so child repaints skip the parent
    startTimerHz (kRefreshHz);
}

void DebugPanel::setBackdropOpacity (float opacity)
{
    backdropOpacity = juce::jlimit (0.0F, 1.0F, opacity);
    setOpaque (backdropOpacity >= 1.0F);
    repaint();
}

void DebugPanel::addMasterControls()
{
    auto& mixer = engine.getMixer();
    crossfader.setRange (0.0, 1.0, 0.001);
    crossfader.setValue (0.5, juce::dontSendNotification);
    crossfader.setDoubleClickReturnValue (true, 0.5);
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
    applyTheme();

    for (juce::Component* c : { static_cast<juce::Component*> (&crossfader), static_cast<juce::Component*> (&master),
                                static_cast<juce::Component*> (&cueMix), static_cast<juce::Component*> (&cueLevel),
                                static_cast<juce::Component*> (&cueLevelLabel), static_cast<juce::Component*> (&meterLabelL),
                                static_cast<juce::Component*> (&meterLabelR), static_cast<juce::Component*> (&crossfaderLabel),
                                static_cast<juce::Component*> (&masterLabel), static_cast<juce::Component*> (&cueLabel),
                                static_cast<juce::Component*> (&masterMeterL), static_cast<juce::Component*> (&masterMeterR) })
        addAndMakeVisible (c);
}

// Theme-dependent bits live here so a theme switch can re-apply them.
void DebugPanel::applyTheme()
{
    for (auto* s : { &crossfader, &master, &cueMix, &cueLevel })
        s->setColour (juce::Slider::trackColourId, theme::master());
    masterLabel.setFont (theme::font (theme::typeLabel(), true));
    masterLabel.setColour (juce::Label::textColourId, theme::textPrimary());
    for (size_t i = 0; i < decks.size(); ++i)
        decks[i]->setAccent (theme::accentFor (static_cast<int> (i)));
}

void DebugPanel::lookAndFeelChanged() { applyTheme(); }

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
    if (! isOpaque())
        g.fillAll (juce::Colours::transparentBlack);
    theme::paintBackdrop (g, getLocalBounds().toFloat(), backdropOpacity);

    // Wordmark: accent bars per deck, then the name.
    auto header = headerArea.toFloat();
    for (int i = 0; i < engine.getNumDecks(); ++i)
    {
        g.setColour (theme::accentFor (i));
        g.fillRoundedRectangle (header.removeFromLeft (4.0F).withSizeKeepingCentre (4.0F, 16.0F), 2.0F);
        header.removeFromLeft (3.0F);
    }
    header.removeFromLeft (8.0F);
    g.setColour (theme::textPrimary());
    g.setFont (theme::font (theme::typeHeading(), true));
    g.drawText ("OpenMixer", header, juce::Justification::centredLeft, false);
    g.setColour (theme::textMuted());
    g.setFont (theme::font (theme::typeCaption()));
    g.drawText (juce::String (engine.getNumDecks()) + "-deck console", header, juce::Justification::centredRight, false);

    theme::paintGlassCard (g, masterCard.toFloat(), theme::master());
    theme::paintGlassCard (g, bottomCard.toFloat());
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
        auto cell = deckRow.removeFromLeft ((deckRow.getWidth() - (n - 1 - i) * theme::gap()) / (n - i));
        decks[static_cast<size_t> (i)]->setBounds (cell);
        deckRow.removeFromLeft (theme::gap());
    }

    bottomCard = area.removeFromBottom (kBottomHeight);
    area.removeFromBottom (theme::gap());
    layoutBottom (bottomCard.reduced (theme::pad(), theme::pad() - 2));
    layoutMixerRow (area);
}

// Channels split around the master section: the first half left of it, the rest right.
void DebugPanel::layoutMixerRow (juce::Rectangle<int> row)
{
    const int n = static_cast<int> (channels.size());
    const int leftCount = (n + 1) / 2;
    const int stripWidth = n > 0 ? (row.getWidth() - kMasterWidth - n * theme::gap()) / n : 0;
    for (int i = 0; i < n; ++i)
    {
        if (i == leftCount)
        {
            masterCard = row.removeFromLeft (kMasterWidth);
            row.removeFromLeft (theme::gap());
        }
        channels[static_cast<size_t> (i)]->setBounds (i == n - 1 ? row : row.removeFromLeft (stripWidth));
        row.removeFromLeft (theme::gap());
    }
    if (leftCount == n)
        masterCard = row;
    layoutMaster (masterCard.reduced (theme::pad()));
}

void DebugPanel::layoutMaster (juce::Rectangle<int> column)
{
    masterLabel.setBounds (column.removeFromTop (22));
    column.removeFromTop (theme::innerGap());
    auto meters = column.removeFromRight (2 * kMasterMeterWidth + 4);
    auto labels = meters.removeFromTop (kCaptionHeight);
    meterLabelL.setBounds (labels.removeFromLeft (kMasterMeterWidth));
    meterLabelR.setBounds (labels.removeFromRight (kMasterMeterWidth));
    masterMeterL.setBounds (meters.removeFromLeft (kMasterMeterWidth));
    masterMeterR.setBounds (meters.removeFromRight (kMasterMeterWidth));
    column.removeFromRight (4);
    master.setBounds (column.withTrimmedTop (kCaptionHeight).withSizeKeepingCentre (std::min (column.getWidth(), kMaxFaderWidth),
                                                                                      column.getHeight() - kCaptionHeight));
}

void DebugPanel::layoutBottom (juce::Rectangle<int> row)
{
    const int side = row.getWidth() / 4;
    placeCaptioned (row.removeFromLeft (side).reduced (4, 0), cueLabel, cueMix);
    placeCaptioned (row.removeFromRight (side).reduced (4, 0), cueLevelLabel, cueLevel);
    placeCaptioned (row.reduced (theme::gap(), 0), crossfaderLabel, crossfader);
}

} // namespace openmixer::ui
