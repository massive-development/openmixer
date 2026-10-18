#include "MeterBar.h"

#include "Theme.h"

#include <algorithm>
#include <cmath>

namespace openmixer::ui
{

namespace
{
    constexpr float kRelease = 0.82F;   // per refresh tick
    constexpr int kHoldTicks = 30;
    constexpr float kSegment = 4.0F, kSegmentGap = 1.0F;
    constexpr float kMidDb = -18.0F, kHighDb = -6.0F;
}

float MeterBar::proportionOf (float linear) noexcept
{
    if (linear <= 0.0F)
        return 0.0F;
    const float db = 20.0F * std::log10 (linear);
    return std::clamp ((db - kFloorDb) / -kFloorDb, 0.0F, 1.0F);
}

void MeterBar::setLevel (float linear)
{
    const float target = proportionOf (linear);
    float next = std::max (target, level * kRelease);
    if (next < 0.002F)
        next = 0.0F;

    float nextPeak = peak;
    if (target >= peak)
    {
        nextPeak = target;
        holdTicks = kHoldTicks;
    }
    else if (holdTicks > 0)
        --holdTicks;
    else
        nextPeak = std::max (next, peak * kRelease);

    if (next == level && nextPeak == peak)
        return;
    level = next;
    peak = nextPeak;
    repaint();
}

void MeterBar::paint (juce::Graphics& g)
{
    using namespace theme;
    const auto bounds = getLocalBounds().toFloat();
    g.setColour (well());
    g.fillRoundedRectangle (bounds, 3.0F);

    const auto inner = bounds.reduced (2.0F);
    const auto yFor = [&inner] (float p) { return inner.getBottom() - inner.getHeight() * p; };
    const float midY = yFor ((kMidDb - kFloorDb) / -kFloorDb), highY = yFor ((kHighDb - kFloorDb) / -kFloorDb);
    const auto colourAt = [midY, highY] (float y) { return y <= highY ? meterHigh() : y <= midY ? meterMid() : meterLow(); };

    // Lit segments up to the level, faint ghosts above so the scale stays readable when silent.
    const float top = yFor (level);
    for (float y = inner.getBottom() - kSegment; y >= inner.getY() - 0.5F; y -= kSegment + kSegmentGap)
    {
        const bool lit = y + kSegment * 0.5F >= top && level > 0.0F;
        g.setColour (colourAt (y).withAlpha (lit ? 0.95F : 0.08F));
        g.fillRect (juce::Rectangle<float> (inner.getX(), y, inner.getWidth(), kSegment));
    }

    if (peak > 0.0F)
    {
        const float py = std::max (inner.getY(), yFor (peak));
        g.setColour (colourAt (py));
        g.fillRect (juce::Rectangle<float> (inner.getX(), py, inner.getWidth(), 2.0F));
    }
}

} // namespace openmixer::ui
