#include "MixerPanel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kPageGroup = 1004, kPageButtonWidth = 80;
    constexpr float kMaxFrameSeconds = 0.25F;   // a stalled UI should not blank the meters in one step
}

MixerPanel::MixerPanel (MixerBinding b) : binding (b), master (b), crossfader (b), cue (b), fxPanel (b)
{
    for (int ch = 0; ch < binding.numChannels(); ++ch)
    {
        strips.push_back (std::make_unique<ChannelStripWidget> (binding, ch));
        strips.back()->onFxRequested = [this] (int channel) { fxPanel.showChannel (channel); setPage (Page::Fx); };
        addAndMakeVisible (*strips.back());
    }
    const char* text[2] { "Mixer", "FX" };
    for (size_t i = 0; i < pageButtons.size(); ++i)
    {
        pageButtons[i].setButtonText (text[i]);
        pageButtons[i].setRadioGroupId (kPageGroup, juce::dontSendNotification);
        pageButtons[i].onClick = [this, i] { setPage (static_cast<Page> (i)); };
        addAndMakeVisible (pageButtons[i]);
    }
    for (juce::Component* c : { static_cast<juce::Component*> (&master), static_cast<juce::Component*> (&crossfader),
                                static_cast<juce::Component*> (&cue), static_cast<juce::Component*> (&fxPanel) })
        addAndMakeVisible (c);
    applyTheme();
    setPage (Page::Mixer);
    lastTickMs = juce::Time::getMillisecondCounterHiRes();
}

void MixerPanel::applyTheme()
{
    for (auto& p : pageButtons)
        p.setColour (juce::TextButton::buttonOnColourId, theme::master());
}

void MixerPanel::setPage (Page p)
{
    page = p;
    pageButtons[static_cast<size_t> (p)].setToggleState (true, juce::dontSendNotification);
    const bool mixerPage = p == Page::Mixer;
    for (auto& s : strips)
        s->setVisible (mixerPage);
    for (juce::Component* c : { static_cast<juce::Component*> (&master), static_cast<juce::Component*> (&crossfader),
                                static_cast<juce::Component*> (&cue) })
        c->setVisible (mixerPage);
    fxPanel.setVisible (! mixerPage);
    fxPanel.refresh();
}

void MixerPanel::tick()
{
    const auto now = juce::Time::getMillisecondCounterHiRes();
    refresh (std::min (static_cast<float> ((now - lastTickMs) / 1000.0), kMaxFrameSeconds));
    lastTickMs = now;
}

void MixerPanel::refresh (float dtSeconds)
{
    if (page == Page::Fx)
        return fxPanel.refresh();
    for (auto& s : strips)
        s->refresh (dtSeconds);
    master.refresh (dtSeconds);
    crossfader.refresh();
    cue.refresh();
}

// Channels split around the master section: the first half left of it, the rest right.
void MixerPanel::layoutStrips (juce::Rectangle<int> row)
{
    const int n = getNumChannels(), leftCount = (n + 1) / 2, gap = theme::innerGap();
    const int stripWidth = n > 0 ? (row.getWidth() - theme::mixerMasterWidth() - n * gap) / n : 0;
    for (int i = 0; i < n; ++i)
    {
        if (i == leftCount)
        {
            master.setBounds (row.removeFromLeft (theme::mixerMasterWidth()));
            row.removeFromLeft (gap);
        }
        strips[static_cast<size_t> (i)]->setBounds (i == n - 1 && leftCount < n ? row : row.removeFromLeft (stripWidth));
        row.removeFromLeft (gap);
    }
    if (leftCount == n)
        master.setBounds (row);
}

void MixerPanel::resized()
{
    auto area = getLocalBounds();
    auto tabs = area.removeFromTop (theme::mixerTabHeight());
    for (auto& p : pageButtons)
        p.setBounds (tabs.removeFromLeft (kPageButtonWidth));
    area.removeFromTop (theme::innerGap());

    fxPanel.setBounds (area);
    cue.setBounds (area.removeFromBottom (theme::mixerCueHeight()));
    area.removeFromBottom (theme::innerGap());
    crossfader.setBounds (area.removeFromBottom (theme::mixerCrossfaderHeight()));
    area.removeFromBottom (theme::innerGap());
    layoutStrips (area);
}

} // namespace openmixer::ui
