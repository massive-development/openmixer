#include "TrackHeader.h"

#include "ClockFormat.h"
#include "Theme.h"

#include <cmath>

namespace openmixer::ui
{

namespace
{
    constexpr int kRowSplitPercent = 55;
}

TrackHeader::TrackHeader (DeckBinding binding) : deckRef (binding)
{
    for (auto* l : { &title, &artist, &bpmLabel, &keyLabel, &elapsed, &remaining })
    {
        l->setBorderSize ({ 0, 0, 0, 0 });
        l->setMinimumHorizontalScale (0.7F);
        l->setInterceptsMouseClicks (false, false);
        addAndMakeVisible (*l);
    }
    for (auto* l : { &bpmLabel, &keyLabel, &elapsed, &remaining })
        l->setJustificationType (juce::Justification::centredRight);
    setTrack ({});
    applyTheme();
}

void TrackHeader::applyTheme()
{
    title.setFont (theme::font (theme::typeTitle(), true));
    artist.setFont (theme::font (theme::typeBody()));
    for (auto* l : { &bpmLabel, &elapsed, &remaining })
        l->setFont (theme::monoFont (theme::typeMono()));
    keyLabel.setFont (theme::font (theme::typeLabel(), true));
    title.setColour (juce::Label::textColourId, theme::textPrimary());
    artist.setColour (juce::Label::textColourId, theme::textSecondary());
    for (auto* l : { &bpmLabel, &elapsed })
        l->setColour (juce::Label::textColourId, theme::textPrimary());
    remaining.setColour (juce::Label::textColourId, theme::textSecondary());
    keyLabel.setColour (juce::Label::textColourId, theme::accentFor (deckRef.index));
}

juce::String TrackHeader::formatTime (double seconds, bool isRemaining)
{
    const auto text = formatClock (seconds, true);
    return isRemaining ? "-" + text : text;
}

juce::String TrackHeader::formatBpm (double bpm) { return bpm > 0.0 ? juce::String (bpm, 1) : juce::String ("--"); }

juce::String TrackHeader::formatKey (const TrackInfo& t)
{
    juce::StringArray parts { t.key, t.camelot };
    parts.removeEmptyStrings();
    return parts.joinIntoString ("  ");
}

void TrackHeader::setIfChanged (juce::Label& label, const juce::String& text)
{
    if (label.getText() != text)   // setText re-lays out the label; skip when nothing changed
        label.setText (text, juce::dontSendNotification);
}

void TrackHeader::setTrack (const TrackInfo& newInfo)
{
    info = newInfo;
    title.setText (info.title.isEmpty() ? "No track loaded" : info.title, juce::dontSendNotification);
    artist.setText (info.artist, juce::dontSendNotification);
    keyLabel.setText (formatKey (info), juce::dontSendNotification);
    refresh();
}

double TrackHeader::effectiveBpm() const
{
    const auto grid = deckRef.control().getGrid();
    return (grid != nullptr ? grid->getBpm() : info.bpm) * deckRef.deck().getRate();
}

void TrackHeader::refresh()
{
    const auto& deck = deckRef.deck();
    const double fileRate = deck.getFileSampleRate();
    const double length = fileRate > 0.0 ? static_cast<double> (deck.getLengthFrames()) / fileRate : 0.0;
    const double position = fileRate > 0.0 ? static_cast<double> (deck.getPositionFrames()) / fileRate : 0.0;
    setIfChanged (elapsed, formatTime (position, false));
    setIfChanged (remaining, formatTime (length - position, true));
    setIfChanged (bpmLabel, formatBpm (effectiveBpm()));
}

void TrackHeader::resized()
{
    auto area = getLocalBounds();
    auto top = area.removeFromTop (area.getHeight() * kRowSplitPercent / 100);
    bpmLabel.setBounds (top.removeFromRight (theme::deckReadoutWidth()));
    title.setBounds (top);
    remaining.setBounds (area.removeFromRight (theme::deckReadoutWidth()));
    elapsed.setBounds (area.removeFromRight (theme::deckReadoutWidth()));
    keyLabel.setBounds (area.removeFromRight (theme::deckReadoutWidth()));
    artist.setBounds (area);
}

} // namespace openmixer::ui
