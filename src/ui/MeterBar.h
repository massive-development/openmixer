// Vertical peak meter: dB scale, LED segments, fast attack / smooth release and a peak-hold tick.
// setLevel() is expected at the UI refresh rate; the ballistics are per call. Message thread only.
#pragma once

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class MeterBar : public juce::Component
{
public:
    static constexpr float kFloorDb = -48.0F;

    MeterBar() { setOpaque (false); }

    void setLevel (float linear);
    void paint (juce::Graphics& g) override;

    // 0..1 bar height for a linear peak (kFloorDb..0 dBFS).
    [[nodiscard]] static float proportionOf (float linear) noexcept;

private:
    float level = 0.0F, peak = 0.0F;
    int holdTicks = 0;
};

} // namespace openmixer::ui
