#include "OverviewWaveform.h"

#include "Theme.h"
#include "WaveformPainter.h"

namespace openmixer::ui
{

namespace
{
    constexpr juce::int64 kNoCue = -1;
}

OverviewWaveform::OverviewWaveform (DeckBinding binding) : deckRef (binding)
{
    shownCues.fill (kNoCue);
    setOpaque (false);
}

void OverviewWaveform::setWaveform (std::shared_ptr<const analysis::WaveformData> data)
{
    waveform = std::move (data);
    cache = {};
    repaint();
}

double OverviewWaveform::seekAt (double x)
{
    const double fraction = wave::xToFraction (x, getWidth());
    deckRef.send ("seek", fraction);
    return fraction;
}

float OverviewWaveform::playheadX() const
{
    return static_cast<float> (wave::frameToX (deckRef.deck().getPositionFrames(), deckRef.deck().getLengthFrames(), getWidth()));
}

juce::Rectangle<int> OverviewWaveform::playheadStrip (float x) const
{
    return { juce::roundToInt (x) - 2, 0, 5, getHeight() };
}

void OverviewWaveform::refresh()
{
    const float x = playheadX();
    if (std::abs (x - shownX) >= 0.5F)
    {
        repaint (playheadStrip (shownX));
        repaint (playheadStrip (x));
        shownX = x;
    }
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
    {
        const auto cue = deckRef.deck().getHotcue (i).value_or (kNoCue);
        if (cue != shownCues[static_cast<size_t> (i)])
        {
            shownCues[static_cast<size_t> (i)] = cue;
            repaint();   // markers are rare events: repaint the strip
        }
    }
}

void OverviewWaveform::renderCache()
{
    cache = juce::Image (juce::Image::ARGB, std::max (1, getWidth()), std::max (1, getHeight()), true);
    juce::Graphics g (cache);
    const auto area = getLocalBounds().toFloat().reduced (0.0F, 2.0F);
    if (waveform != nullptr && waveform->isValid())
        wave::paintColumns (g, *waveform, area, 0.0, static_cast<double> (waveform->getNumSamples()));
}

void OverviewWaveform::paintMarkers (juce::Graphics& g) const
{
    const auto length = deckRef.deck().getLengthFrames();
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
    {
        const auto cue = shownCues[static_cast<size_t> (i)];
        if (cue == kNoCue)
            continue;
        g.setColour (theme::accentFor (i));
        const auto x = static_cast<float> (wave::frameToX (cue, length, getWidth()));
        g.fillRect (x - theme::cueMarkerWidth() * 0.5F, 0.0F, theme::cueMarkerWidth(), static_cast<float> (getHeight()));
    }
}

void OverviewWaveform::paint (juce::Graphics& g)
{
    const auto bounds = getLocalBounds().toFloat();
    g.setColour (theme::well());
    g.fillRoundedRectangle (bounds, theme::controlRadius());
    if (waveform == nullptr || ! waveform->isValid())
    {
        wave::paintEmpty (g, bounds, "No waveform");
        return;
    }
    if (! cache.isValid() || cache.getWidth() != getWidth() || cache.getHeight() != getHeight())
        renderCache();
    g.drawImageAt (cache, 0, 0);
    paintMarkers (g);
    g.setColour (theme::accentFor (deckRef.index));
    g.fillRect (juce::Rectangle<float> (theme::playheadWidth(), bounds.getHeight()).withCentre ({ playheadX(), bounds.getCentreY() }));
}

} // namespace openmixer::ui
