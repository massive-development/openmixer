// Crossfader slider with its curve selector (Linear / Power). Routes "mixer.crossfader" and
// "mixer.crossfader.curve"; the selector mirrors the engine. Message thread only.
#pragma once

#include "ControlCaption.h"
#include "MixerBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>

namespace openmixer::ui
{

class CrossfaderSection : public juce::Component
{
public:
    explicit CrossfaderSection (MixerBinding binding);

    void refresh();

    juce::Slider& getSlider() noexcept { return slider; }
    juce::TextButton& getCurveButton (CrossfaderCurve c) noexcept { return curves[static_cast<size_t> (c)]; }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();

    MixerBinding binding;
    juce::Label caption, curveCaption;
    juce::Slider slider { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    std::array<juce::TextButton, 2> curves;   // indexed by CrossfaderCurve
};

} // namespace openmixer::ui
