// One mixer channel as a glass card: an aligned knob row (Trim, Hi, Mid, Lo, Filter), then
// PFL + crossfader assign beside the channel fader and its meter. Message thread only.
#pragma once

#include "../engine/Mixer.h"
#include "ControlCaption.h"
#include "MeterBar.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>
#include <functional>

namespace openmixer::ui
{

class ChannelStripPanel : public juce::Component
{
public:
    ChannelStripPanel (Mixer& mixer, int channel);

    void refresh();
    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override;

private:
    void applyTheme();
    void addKnob (juce::Slider& s, juce::Label& caption, const juce::String& text, const juce::String& id,
                  double lo, double hi, double initial, std::function<juce::String (double)> format);
    void addAssignButtons();
    void layoutKnobs (juce::Rectangle<int> area);
    void layoutFaderZone (juce::Rectangle<int> area);

    Mixer& mixer;
    int channel;
    juce::Colour accent;
    std::array<juce::Slider, 3> eq;
    juce::Slider trim, filter, fader;
    std::array<juce::Label, 3> eqCaptions;
    juce::Label titleLabel, trimCaption, filterCaption, faderCaption, sideCaption, meterCaption;
    juce::TextButton pfl { "PFL" };
    std::array<juce::TextButton, 3> assign;   // indexed by CrossfaderAssign
    MeterBar meter;
};

} // namespace openmixer::ui
