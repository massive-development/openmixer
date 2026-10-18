#include "CueSection.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace { constexpr double kMaxCueLevel = 1.5; }

CueSection::CueSection (MixerBinding b) : binding (b)
{
    styleCaption (mixCaption, "Cue / Master");
    styleCaption (levelCaption, "Phones level");
    mix.setTooltip ("Headphone blend: cue channels only (left) to master only (right)");
    mix.setRange (0.0, 1.0, 0.001);
    mix.setDoubleClickReturnValue (true, 0.0);
    mix.onValueChange = [this] { binding.send ("cue.mix", mix.getValue()); };
    level.setRange (0.0, kMaxCueLevel, 0.001);
    level.setValue (1.0, juce::dontSendNotification);
    level.setDoubleClickReturnValue (true, 1.0);
    level.onValueChange = [this] { binding.send ("cue.level", level.getValue()); };
    for (juce::Component* c : { static_cast<juce::Component*> (&mixCaption), static_cast<juce::Component*> (&levelCaption),
                                static_cast<juce::Component*> (&mix), static_cast<juce::Component*> (&level) })
        addAndMakeVisible (c);
    applyTheme();
}

void CueSection::applyTheme()
{
    mix.setColour (juce::Slider::trackColourId, theme::master());
    level.setColour (juce::Slider::trackColourId, theme::master());
}

void CueSection::refresh()
{
    auto& m = binding.mixer();
    if (! mix.isMouseButtonDown())
        mix.setValue (m.getCueMix(), juce::dontSendNotification);
    if (! level.isMouseButtonDown())
        level.setValue (m.getCueLevel(), juce::dontSendNotification);
}

void CueSection::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F));
}

void CueSection::resized()
{
    auto area = getLocalBounds().reduced (theme::pad(), theme::pad() - 4);
    const int half = (area.getWidth() - theme::gap()) / 2;
    placeCaptioned (area.removeFromLeft (half), mixCaption, mix);
    area.removeFromLeft (theme::gap());
    placeCaptioned (area, levelCaption, level);
}

} // namespace openmixer::ui
