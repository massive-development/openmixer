#include "DeckStrip.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kRateCaptionWidth = 40;
}

DeckStrip::DeckStrip (Deck& d, const juce::String& title) : deck (d)
{
    titleLabel.setText (title, juce::dontSendNotification);
    rateSlider.setRange (0.5, 1.5, 0.001);
    rateSlider.setValue (1.0, juce::dontSendNotification);
    rateSlider.onValueChange = [this] { deck.setRate (rateSlider.getValue()); };
    loadButton.onClick = [this] { chooseFile(); };
    playButton.onClick = [this] { deck.setPlaying (! deck.isPlaying()); };

    styleCaption (rateCaption, "Rate");
    rateCaption.setJustificationType (juce::Justification::centredLeft);
    positionLabel.setFont (juce::Font (juce::FontOptions (juce::Font::getDefaultMonospacedFontName(), 13.0F, juce::Font::plain)));
    positionLabel.setMinimumHorizontalScale (0.6F);
    positionLabel.setJustificationType (juce::Justification::centredRight);

    addAndMakeVisible (rateCaption);
    addAndMakeVisible (titleLabel);
    addAndMakeVisible (positionLabel);
    addAndMakeVisible (loadButton);
    addAndMakeVisible (playButton);
    addAndMakeVisible (rateSlider);
}

// Fixed-width fields so the readout never changes width between ticks.
juce::String DeckStrip::formatPosition (double seconds, double total)
{
    return juce::String (seconds, 1).paddedLeft (' ', 7) + " / " + juce::String (total, 1).paddedLeft (' ', 7) + " s";
}

void DeckStrip::chooseFile()
{
    if (choosing)
        return;   // replacing a live chooser would close and reopen its dialog
    choosing = true;
    chooser = std::make_unique<juce::FileChooser> ("Load audio file", juce::File(), "*.wav;*.aif;*.aiff;*.flac;*.mp3;*.ogg");
    chooser->launchAsync (juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                          [this] (const juce::FileChooser& fc)
                          {
                              choosing = false;
                              if (fc.getResult().existsAsFile())
                                  deck.load (fc.getResult());
                          });
}

void DeckStrip::refresh()
{
    playButton.setButtonText (deck.isPlaying() ? "Pause" : "Play");
    const double rate = deck.getFileSampleRate();
    const double seconds = rate > 0.0 ? static_cast<double> (deck.getPositionFrames()) / rate : 0.0;
    const double total = rate > 0.0 ? static_cast<double> (deck.getLengthFrames()) / rate : 0.0;
    positionLabel.setText (formatPosition (seconds, total), juce::dontSendNotification);
}

void DeckStrip::resized()
{
    auto area = getLocalBounds().reduced (4);
    titleLabel.setBounds (area.removeFromTop (24));
    auto row = area.removeFromTop (28);
    loadButton.setBounds (row.removeFromLeft (70));
    playButton.setBounds (row.removeFromLeft (74).withTrimmedLeft (4));
    positionLabel.setBounds (row.withTrimmedLeft (8));
    auto rateRow = area.removeFromTop (28);
    rateCaption.setBounds (rateRow.removeFromLeft (kRateCaptionWidth));
    rateSlider.setBounds (rateRow);
}

} // namespace openmixer::ui
