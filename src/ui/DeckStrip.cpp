#include "DeckStrip.h"

namespace openmixer::ui
{

DeckStrip::DeckStrip (Deck& d, const juce::String& title) : deck (d)
{
    titleLabel.setText (title, juce::dontSendNotification);
    rateSlider.setRange (0.5, 1.5, 0.001);
    rateSlider.setValue (1.0, juce::dontSendNotification);
    rateSlider.onValueChange = [this] { deck.setRate (rateSlider.getValue()); };
    loadButton.onClick = [this] { chooseFile(); };
    playButton.onClick = [this] { deck.setPlaying (! deck.isPlaying()); };

    addAndMakeVisible (titleLabel);
    addAndMakeVisible (positionLabel);
    addAndMakeVisible (loadButton);
    addAndMakeVisible (playButton);
    addAndMakeVisible (rateSlider);
}

void DeckStrip::chooseFile()
{
    chooser = std::make_unique<juce::FileChooser> ("Load audio file", juce::File(), "*.wav;*.aif;*.aiff;*.flac;*.mp3;*.ogg");
    chooser->launchAsync (juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                          [this] (const juce::FileChooser& fc)
                          {
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
    positionLabel.setText (juce::String (seconds, 1) + " / " + juce::String (total, 1) + " s", juce::dontSendNotification);
}

void DeckStrip::resized()
{
    auto area = getLocalBounds().reduced (4);
    titleLabel.setBounds (area.removeFromTop (24));
    auto row = area.removeFromTop (28);
    loadButton.setBounds (row.removeFromLeft (70));
    playButton.setBounds (row.removeFromLeft (74).withTrimmedLeft (4));
    positionLabel.setBounds (row.withTrimmedLeft (8));
    rateSlider.setBounds (area.removeFromTop (28));
}

} // namespace openmixer::ui
