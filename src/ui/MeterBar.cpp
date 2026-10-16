#include "MeterBar.h"

#include <algorithm>

namespace openmixer::ui
{

void MeterBar::setLevel (float linear)
{
    const float clamped = std::clamp (linear, 0.0F, 1.0F);
    if (clamped == level)
        return;
    level = clamped;
    repaint();
}

void MeterBar::paint (juce::Graphics& g)
{
    const auto bounds = getLocalBounds().toFloat();
    g.setColour (juce::Colours::black);
    g.fillRect (bounds);
    g.setColour (level > 0.9F ? juce::Colours::red : juce::Colours::limegreen);
    g.fillRect (bounds.withTop (bounds.getBottom() - bounds.getHeight() * level));
}

} // namespace openmixer::ui
