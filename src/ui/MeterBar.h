// Minimal vertical level bar (linear 0..1).
#pragma once

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class MeterBar : public juce::Component
{
public:
    void setLevel (float linear);
    void paint (juce::Graphics& g) override;

private:
    float level = 0.0F;
};

} // namespace openmixer::ui
