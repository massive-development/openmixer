#include "ChannelStripPanel.h"

namespace openmixer::ui
{

ChannelStripPanel::ChannelStripPanel (Mixer& m, int ch) : mixer (m), channel (ch)
{
    const auto prefix = "ch" + juce::String (channel) + ".";
    const char* eqNames[3] { "eq.low", "eq.mid", "eq.high" };
    for (size_t i = 0; i < eq.size(); ++i)
        addKnob (eq[i], prefix + eqNames[i], Mixer::kMinEqDb, Mixer::kMaxEqDb, 0.0);
    addKnob (filter, prefix + "filter", -1.0, 1.0, 0.0);

    fader.setSliderStyle (juce::Slider::LinearVertical);
    fader.setTextBoxStyle (juce::Slider::NoTextBox, false, 0, 0);
    fader.setRange (0.0, 1.0, 0.001);
    fader.setValue (1.0, juce::dontSendNotification);
    fader.onValueChange = [this, prefix] { mixer.setParameter (prefix + "fader", fader.getValue()); };
    addAndMakeVisible (fader);

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

void ChannelStripPanel::addKnob (juce::Slider& s, const juce::String& id, double lo, double hi, double initial)
{
    s.setSliderStyle (juce::Slider::RotaryHorizontalVerticalDrag);
    s.setTextBoxStyle (juce::Slider::TextBoxBelow, false, 50, 16);
    s.setRange (lo, hi, 0.01);
    s.setValue (initial, juce::dontSendNotification);
    s.setDoubleClickReturnValue (true, initial);
    s.onValueChange = [this, &s, id] { mixer.setParameter (id, s.getValue()); };
    addAndMakeVisible (s);
}

void ChannelStripPanel::refresh()
{
    meter.setLevel (mixer.getChannelPeak (channel));
}

void ChannelStripPanel::resized()
{
    auto area = getLocalBounds().reduced (4);
    for (auto* knob : { &eq[2], &eq[1], &eq[0], &filter })
        knob->setBounds (area.removeFromTop (64));
    side.setBounds (area.removeFromTop (24));
    pfl.setBounds (area.removeFromTop (24));
    meter.setBounds (area.removeFromRight (14));
    fader.setBounds (area);
}

} // namespace openmixer::ui
