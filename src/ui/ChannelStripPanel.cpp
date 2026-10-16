#include "ChannelStripPanel.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kKnobHeight = 84, kKnobTextWidth = 54, kKnobTextHeight = 16;
    constexpr int kTitleHeight = 18, kButtonHeight = 24;
    constexpr int kMeterColumnWidth = 28, kMeterWidth = 14;
}

ChannelStripPanel::ChannelStripPanel (Mixer& m, int ch) : mixer (m), channel (ch)
{
    const auto prefix = "ch" + juce::String (channel) + ".";
    const char* eqIds[3] { "eq.low", "eq.mid", "eq.high" };
    const char* eqText[3] { "Lo", "Mid", "Hi" };
    for (size_t i = 0; i < eq.size(); ++i)
        addKnob (eq[i], eqCaptions[i], eqText[i], prefix + eqIds[i], Mixer::kMinEqDb, Mixer::kMaxEqDb, 0.0);
    addKnob (filter, filterCaption, "Filter", prefix + "filter", -1.0, 1.0, 0.0);

    styleCaption (titleLabel, "Channel " + juce::String (channel + 1));
    titleLabel.setFont (juce::Font (juce::FontOptions (14.0F, juce::Font::bold)));
    styleCaption (faderCaption, "Fader");
    styleCaption (sideCaption, "X-fader");
    styleCaption (meterCaption, "Lvl");
    for (auto* l : { &titleLabel, &faderCaption, &sideCaption, &meterCaption })
        addAndMakeVisible (*l);

    fader.setSliderStyle (juce::Slider::LinearVertical);
    fader.setTextBoxStyle (juce::Slider::NoTextBox, false, 0, 0);
    fader.setRange (0.0, 1.0, 0.001);
    fader.setValue (1.0, juce::dontSendNotification);
    fader.onValueChange = [this, prefix] { mixer.setParameter (prefix + "fader", fader.getValue()); };
    addAndMakeVisible (fader);
    addKnob (trim, trimCaption, "Trim", prefix + "trim", 0.0, 4.0, 1.0);   // after fader: keeps child order stable

    pfl.onClick = [this] { mixer.setPfl (channel, pfl.getToggleState()); };
    addAndMakeVisible (pfl);

    side.addItem ("Thru", 1);
    side.addItem ("Left", 2);
    side.addItem ("Right", 3);
    side.setSelectedId (1, juce::dontSendNotification);
    side.onChange = [this] { mixer.setCrossfaderAssign (channel, static_cast<CrossfaderAssign> (side.getSelectedId() - 1)); };
    addAndMakeVisible (side);
    addAndMakeVisible (meter);
}

void ChannelStripPanel::addKnob (juce::Slider& s, juce::Label& caption, const juce::String& text, const juce::String& id,
                                 double lo, double hi, double initial)
{
    s.setSliderStyle (juce::Slider::RotaryHorizontalVerticalDrag);
    s.setTextBoxStyle (juce::Slider::TextBoxBelow, false, kKnobTextWidth, kKnobTextHeight);
    s.setRange (lo, hi, 0.01);
    s.setValue (initial, juce::dontSendNotification);
    s.setDoubleClickReturnValue (true, initial);
    s.onValueChange = [this, &s, id] { mixer.setParameter (id, s.getValue()); };
    styleCaption (caption, text);
    addAndMakeVisible (s);
    addAndMakeVisible (caption);
}

void ChannelStripPanel::refresh()
{
    meter.setLevel (mixer.getChannelPeak (channel));
}

void ChannelStripPanel::layoutKnobs (juce::Rectangle<int>& area)
{
    auto eqRow = area.removeFromTop (kCaptionHeight + kKnobHeight);
    const int cell = eqRow.getWidth() / 3;
    for (size_t i : { size_t { 2 }, size_t { 1 }, size_t { 0 } })   // Hi, Mid, Lo left to right
        placeCaptioned (eqRow.removeFromLeft (cell), eqCaptions[i], eq[i]);

    auto row = area.removeFromTop (kCaptionHeight + kKnobHeight);
    const int half = row.getWidth() / 2;
    placeCaptioned (row.removeFromLeft (half), trimCaption, trim);
    placeCaptioned (row, filterCaption, filter);
}

void ChannelStripPanel::resized()
{
    auto area = getLocalBounds().reduced (4);
    titleLabel.setBounds (area.removeFromTop (kTitleHeight));
    layoutKnobs (area);

    auto row = area.removeFromTop (kCaptionHeight + kButtonHeight);
    const int half = row.getWidth() / 2;
    placeCaptioned (row.removeFromLeft (half).reduced (2, 0), sideCaption, side);
    pfl.setBounds (row.removeFromBottom (kButtonHeight).reduced (2, 0));

    auto meterColumn = area.removeFromRight (kMeterColumnWidth);
    meterCaption.setBounds (meterColumn.removeFromTop (kCaptionHeight));
    meter.setBounds (meterColumn.withSizeKeepingCentre (kMeterWidth, meterColumn.getHeight()));
    placeCaptioned (area, faderCaption, fader);
}

} // namespace openmixer::ui
