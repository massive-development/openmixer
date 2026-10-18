// A rotary knob with a caption above it and a value readout below. Drag or double-click resets;
// onChange fires only for user edits, setValueQuiet() mirrors engine state without echoing.
#pragma once

#include "ControlCaption.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <functional>

namespace openmixer::ui
{

class LabeledKnob : public juce::Component
{
public:
    struct Spec
    {
        juce::String caption;
        double lo = 0.0, hi = 1.0, initial = 0.0;
        std::function<juce::String (double)> format;
    };

    explicit LabeledKnob (Spec spec);

    std::function<void (double)> onChange;

    void setValueQuiet (double v);   // ignored while the user is dragging
    double getValue() const { return slider.getValue(); }
    juce::Slider& getSlider() noexcept { return slider; }
    juce::Label& getCaption() noexcept { return caption; }
    void setAccent (juce::Colour c);

    void resized() override;
    void lookAndFeelChanged() override;

private:
    void applyTheme();

    juce::Slider slider;
    juce::Label caption;
};

} // namespace openmixer::ui
