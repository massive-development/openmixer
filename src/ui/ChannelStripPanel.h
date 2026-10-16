// Debug controls for one mixer channel: EQ, filter, fader, PFL, crossfader side, meter.
#pragma once

#include "../engine/Mixer.h"
#include "ControlCaption.h"
#include "MeterBar.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>

namespace openmixer::ui
{

class ChannelStripPanel : public juce::Component
{
public:
    ChannelStripPanel (Mixer& mixer, int channel);

    void refresh();
    void resized() override;

private:
    void addKnob (juce::Slider& s, juce::Label& caption, const juce::String& text, const juce::String& id,
                  double lo, double hi, double initial);
    void layoutKnobs (juce::Rectangle<int>& area);

    Mixer& mixer;
    int channel;
    std::array<juce::Slider, 3> eq;
    juce::Slider trim, filter, fader;
    std::array<juce::Label, 3> eqCaptions;
    juce::Label titleLabel, trimCaption, filterCaption, faderCaption, sideCaption, meterCaption;
    juce::ToggleButton pfl { "PFL" };
    juce::ComboBox side;
    MeterBar meter;
};

} // namespace openmixer::ui
