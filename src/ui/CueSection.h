// Headphone cue controls: cue/master mix and cue level, routed through "mixer.cue.mix" and
// "mixer.cue.level". Message thread only.
#pragma once

#include "ControlCaption.h"
#include "MixerBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class CueSection : public juce::Component
{
public:
    explicit CueSection (MixerBinding binding);

    void refresh();

    juce::Slider& getMix() noexcept { return mix; }
    juce::Slider& getLevel() noexcept { return level; }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();

    MixerBinding binding;
    juce::Label mixCaption, levelCaption;
    juce::Slider mix { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    juce::Slider level { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
};

} // namespace openmixer::ui
