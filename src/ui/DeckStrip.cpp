#include "DeckStrip.h"

#include "ValueFormat.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kRowHeight = 28, kRateCaptionWidth = 40, kRateTextWidth = 56;
    constexpr int kLoadWidth = 64, kPlayWidth = 76, kProgressHeight = 3;
}

DeckStrip::DeckStrip (Deck& d, const juce::String& title, juce::Colour accentColour) : deck (d), accent (accentColour)
{
    titleLabel.setText (title, juce::dontSendNotification);
    titleLabel.setFont (theme::font (16.0F, true));
    titleLabel.setColour (juce::Label::textColourId, theme::textPrimary);
    titleLabel.setBorderSize ({ 0, 0, 0, 0 });

    rateSlider.textFromValueFunction = format::rate;   // before setValue, which renders the text
    rateSlider.setTextBoxStyle (juce::Slider::TextBoxRight, true, kRateTextWidth, kRowHeight);
    rateSlider.setRange (0.5, 1.5, 0.001);
    rateSlider.setValue (1.0, juce::dontSendNotification);
    rateSlider.setDoubleClickReturnValue (true, 1.0);
    rateSlider.setColour (juce::Slider::trackColourId, accent);
    rateSlider.onValueChange = [this] { deck.setRate (rateSlider.getValue()); };

    loadButton.onClick = [this] { chooseFile(); };
    playButton.onClick = [this] { deck.setPlaying (! deck.isPlaying()); };
    playButton.setColour (juce::TextButton::buttonOnColourId, accent);

    styleCaption (rateCaption, "Rate");
    rateCaption.setJustificationType (juce::Justification::centredLeft);
    positionLabel.setFont (theme::monoFont (15.0F));
    positionLabel.setColour (juce::Label::textColourId, theme::textPrimary);
    positionLabel.setMinimumHorizontalScale (0.6F);
    positionLabel.setJustificationType (juce::Justification::centredRight);
    positionLabel.setBorderSize ({ 0, 0, 0, 0 });

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
    const bool playing = deck.isPlaying();
    playButton.setButtonText (playing ? "Pause" : "Play");
    playButton.setToggleState (playing, juce::dontSendNotification);
    const double rate = deck.getFileSampleRate();
    const double seconds = rate > 0.0 ? static_cast<double> (deck.getPositionFrames()) / rate : 0.0;
    const double total = rate > 0.0 ? static_cast<double> (deck.getLengthFrames()) / rate : 0.0;
    positionLabel.setText (formatPosition (seconds, total), juce::dontSendNotification);

    const float next = total > 0.0 ? static_cast<float> (juce::jlimit (0.0, 1.0, seconds / total)) : 0.0F;
    if (std::abs (next - progress) > 0.0005F)
    {
        progress = next;
        repaint (progressTrack().getSmallestIntegerContainer().expanded (1));
    }
}

juce::Rectangle<float> DeckStrip::progressTrack() const
{
    const auto r = getLocalBounds().toFloat().reduced (static_cast<float> (theme::kPad), 0.0F);
    return r.withTop (r.getBottom() - static_cast<float> (theme::kPad) * 0.5F - kProgressHeight).withHeight (kProgressHeight);
}

void DeckStrip::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F), accent);
    const auto track = progressTrack();
    g.setColour (theme::well);
    g.fillRoundedRectangle (track, 1.5F);
    if (progress > 0.0F)
    {
        g.setColour (accent);
        g.fillRoundedRectangle (track.withWidth (track.getWidth() * progress), 1.5F);
    }
}

void DeckStrip::resized()
{
    auto area = getLocalBounds().reduced (theme::kPad, theme::kPad - 2);
    auto top = area.removeFromTop (kRowHeight);
    titleLabel.setBounds (top.removeFromLeft (top.getWidth() / 3));
    positionLabel.setBounds (top.withTrimmedLeft (theme::kInnerGap));

    area.removeFromTop (theme::kInnerGap - 2);
    auto row = area.removeFromTop (kRowHeight);
    loadButton.setBounds (row.removeFromLeft (kLoadWidth));
    row.removeFromLeft (theme::kInnerGap);
    playButton.setBounds (row.removeFromLeft (kPlayWidth));
    row.removeFromLeft (theme::kGap);
    rateCaption.setBounds (row.removeFromLeft (kRateCaptionWidth));
    rateSlider.setBounds (row);
}

} // namespace openmixer::ui
