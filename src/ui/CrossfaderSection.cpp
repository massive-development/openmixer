#include "CrossfaderSection.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace { constexpr int kCurveGroup = 1002, kCurveWidth = 128; }

CrossfaderSection::CrossfaderSection (MixerBinding b) : binding (b)
{
    styleCaption (caption, "Crossfader");
    styleCaption (curveCaption, "Curve");
    slider.setRange (0.0, 1.0, 0.001);
    slider.setValue (0.5, juce::dontSendNotification);
    slider.setDoubleClickReturnValue (true, 0.5);
    slider.onValueChange = [this] { binding.send ("crossfader", slider.getValue()); };

    const char* text[2] { "Linear", "Power" };
    const char* tips[2] { "Straight blend between the two sides", "Constant-power blend: no volume dip in the middle" };
    for (size_t i = 0; i < curves.size(); ++i)
    {
        curves[i].setButtonText (text[i]);
        curves[i].setTooltip (tips[i]);
        curves[i].setRadioGroupId (kCurveGroup, juce::dontSendNotification);
        curves[i].onClick = [this, i] { binding.send ("crossfader.curve", static_cast<double> (i)); refresh(); };
        addAndMakeVisible (curves[i]);
    }
    curves[0].setConnectedEdges (juce::Button::ConnectedOnRight);
    curves[1].setConnectedEdges (juce::Button::ConnectedOnLeft);
    for (juce::Component* c : { static_cast<juce::Component*> (&caption), static_cast<juce::Component*> (&curveCaption),
                                static_cast<juce::Component*> (&slider) })
        addAndMakeVisible (c);
    applyTheme();
    refresh();
}

void CrossfaderSection::applyTheme()
{
    slider.setColour (juce::Slider::trackColourId, theme::master());
    for (auto& c : curves)
        c.setColour (juce::TextButton::buttonOnColourId, theme::master());
}

void CrossfaderSection::refresh()
{
    auto& m = binding.mixer();
    if (! slider.isMouseButtonDown())
        slider.setValue (m.getCrossfader(), juce::dontSendNotification);
    curves[static_cast<size_t> (m.getCrossfaderCurve())].setToggleState (true, juce::dontSendNotification);
}

void CrossfaderSection::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F));
}

void CrossfaderSection::resized()
{
    auto area = getLocalBounds().reduced (theme::pad(), theme::pad() - 4);
    auto right = area.removeFromRight (kCurveWidth);
    curveCaption.setBounds (right.removeFromTop (kCaptionHeight));
    auto segments = right.removeFromTop (theme::mixerButtonHeight());
    curves[0].setBounds (segments.removeFromLeft (segments.getWidth() / 2));
    curves[1].setBounds (segments);
    area.removeFromRight (theme::gap());
    placeCaptioned (area, caption, slider);
}

} // namespace openmixer::ui
