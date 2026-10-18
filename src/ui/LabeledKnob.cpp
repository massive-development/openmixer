#include "LabeledKnob.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace { constexpr int kTextHeight = 14; }

LabeledKnob::LabeledKnob (Spec spec)
{
    slider.setSliderStyle (juce::Slider::RotaryHorizontalVerticalDrag);
    slider.textFromValueFunction = spec.format ? std::move (spec.format) : [] (double v) { return juce::String (v, 2); };
    slider.setRange (spec.lo, spec.hi);   // continuous: the engine clamps and smooths
    slider.setValue (spec.initial, juce::dontSendNotification);
    slider.setDoubleClickReturnValue (true, spec.initial);
    slider.onValueChange = [this] { if (onChange) onChange (slider.getValue()); };
    styleCaption (caption, spec.caption);
    addAndMakeVisible (slider);
    addAndMakeVisible (caption);
    applyTheme();
}

void LabeledKnob::applyTheme()
{
    caption.setFont (theme::font (theme::typeCaption()));
    caption.setColour (juce::Label::textColourId, theme::textSecondary());
}

void LabeledKnob::lookAndFeelChanged() { applyTheme(); }

void LabeledKnob::setAccent (juce::Colour c) { slider.setColour (juce::Slider::rotarySliderFillColourId, c); }

void LabeledKnob::setValueQuiet (double v)
{
    if (! slider.isMouseButtonDown())
        slider.setValue (v, juce::dontSendNotification);
}

void LabeledKnob::resized()
{
    slider.setTextBoxStyle (juce::Slider::TextBoxBelow, true, getWidth(), kTextHeight);
    placeCaptioned (getLocalBounds(), caption, slider);
}

} // namespace openmixer::ui
