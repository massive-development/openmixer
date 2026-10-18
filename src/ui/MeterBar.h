// Vertical peak meter: dB scale, LED segments, release ballistics, a peak-hold tick and a clip lamp
// at the top (click to clear). All metrics and thresholds come from theme tokens. Message thread only.
#pragma once

#include "MeterBallistics.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class MeterBar : public juce::Component
{
public:
    MeterBar() { setOpaque (false); }

    // One UI frame: linear peak from the engine and the time since the last call.
    void update (float linearPeak, float dtSeconds);

    const MeterBallistics& getBallistics() const noexcept { return ballistics; }
    void paint (juce::Graphics& g) override;
    void mouseDown (const juce::MouseEvent&) override;
    void lookAndFeelChanged() override { ballistics.setParams (MeterParams::fromTheme()); }

private:
    MeterBallistics ballistics;
};

// Left and right bars side by side.
class StereoMeter : public juce::Component
{
public:
    StereoMeter();
    void update (float left, float right, float dtSeconds);
    void resized() override;

    MeterBar& getLeft() noexcept { return bars[0]; }
    MeterBar& getRight() noexcept { return bars[1]; }

private:
    MeterBar bars[2];
};

} // namespace openmixer::ui
