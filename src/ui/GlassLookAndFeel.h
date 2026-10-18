// Glass LookAndFeel (Todo 25): ring knobs, capped faders and pill buttons drawn from theme tokens.
// Per-control accents come from colour ids: Slider::rotarySliderFillColourId / trackColourId and
// TextButton::buttonOnColourId. Message thread only.
#pragma once

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class GlassLookAndFeel : public juce::LookAndFeel_V4
{
public:
    GlassLookAndFeel();

    void drawRotarySlider (juce::Graphics&, int x, int y, int width, int height, float sliderPos,
                           float rotaryStartAngle, float rotaryEndAngle, juce::Slider&) override;
    void drawLinearSlider (juce::Graphics&, int x, int y, int width, int height, float sliderPos,
                           float minSliderPos, float maxSliderPos, juce::Slider::SliderStyle, juce::Slider&) override;
    int getSliderThumbRadius (juce::Slider&) override;
    juce::Label* createSliderTextBox (juce::Slider&) override;

    void drawButtonBackground (juce::Graphics&, juce::Button&, const juce::Colour& backgroundColour,
                               bool isHighlighted, bool isDown) override;
    juce::Font getTextButtonFont (juce::TextButton&, int buttonHeight) override;

    void drawDocumentWindowTitleBar (juce::DocumentWindow&, juce::Graphics&, int w, int h, int titleSpaceX,
                                     int titleSpaceW, const juce::Image* icon, bool drawTitleTextOnLeft) override;

    // Where a control's value arc/fill starts: its reset value when it has one, else its minimum.
    static double originOf (const juce::Slider& s);
};

} // namespace openmixer::ui
