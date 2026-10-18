// Master bus: output level fader and a stereo peak meter, routed through "mixer.master". Message thread only.
#pragma once

#include "ControlCaption.h"
#include "MeterBar.h"
#include "MixerBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class MasterSection : public juce::Component
{
public:
    explicit MasterSection (MixerBinding binding);

    void refresh (float dtSeconds);   // mirrors the engine gain and feeds the meter

    juce::Slider& getFader() noexcept { return fader; }
    StereoMeter& getMeter() noexcept { return meter; }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();

    MixerBinding binding;
    juce::Label title, faderCaption, meterCaption;
    juce::Slider fader { juce::Slider::LinearVertical, juce::Slider::NoTextBox };
    StereoMeter meter;
};

} // namespace openmixer::ui
