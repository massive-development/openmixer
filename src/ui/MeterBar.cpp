#include "MeterBar.h"

#include "Theme.h"

#include <algorithm>

namespace openmixer::ui
{

void MeterBar::update (float linearPeak, float dtSeconds)
{
    if (ballistics.update (linearPeak, dtSeconds))
        repaint();
}

void MeterBar::mouseDown (const juce::MouseEvent&)
{
    ballistics.resetClip();
    repaint();
}

void MeterBar::paint (juce::Graphics& g)
{
    using namespace theme;
    const auto bounds = getLocalBounds().toFloat();
    g.setColour (well());
    g.fillRoundedRectangle (bounds, 3.0F);

    const float seg = meterSegment(), gap = meterSegmentGap();
    auto inner = bounds.reduced (2.0F);
    g.setColour (meterHigh().withAlpha (ballistics.isClipping() ? 0.95F : 0.12F));   // clip lamp
    g.fillRect (inner.removeFromTop (seg));
    inner.removeFromTop (gap);

    const float floorDb = meterFloorDb();
    const auto yFor = [&inner] (float p) { return inner.getBottom() - inner.getHeight() * p; };
    const auto yForDb = [&] (float db) { return yFor ((db - floorDb) / -floorDb); };
    const float midY = yForDb (meterMidDb()), highY = yForDb (meterHighDb());
    const auto colourAt = [midY, highY] (float y) { return y <= highY ? meterHigh() : y <= midY ? meterMid() : meterLow(); };

    const float level = ballistics.level(), top = yFor (level);
    for (float y = inner.getBottom() - seg; y >= inner.getY() - 0.5F; y -= seg + gap)
    {
        const bool lit = y + seg * 0.5F >= top && level > 0.0F;
        g.setColour (colourAt (y).withAlpha (lit ? 0.95F : 0.08F));
        g.fillRect (juce::Rectangle<float> (inner.getX(), y, inner.getWidth(), seg));
    }
    if (ballistics.peakHold() > 0.0F)
    {
        const float py = std::max (inner.getY(), yFor (ballistics.peakHold()));
        g.setColour (colourAt (py));
        g.fillRect (juce::Rectangle<float> (inner.getX(), py, inner.getWidth(), 2.0F));
    }
}

StereoMeter::StereoMeter()
{
    for (auto& b : bars)
        addAndMakeVisible (b);
}

void StereoMeter::update (float left, float right, float dtSeconds)
{
    bars[0].update (left, dtSeconds);
    bars[1].update (right, dtSeconds);
}

void StereoMeter::resized()
{
    auto area = getLocalBounds();
    const int half = (area.getWidth() - 2) / 2;
    bars[0].setBounds (area.removeFromLeft (half));
    area.removeFromLeft (2);
    bars[1].setBounds (area.removeFromLeft (half));
}

} // namespace openmixer::ui
