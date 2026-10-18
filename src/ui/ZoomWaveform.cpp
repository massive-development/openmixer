#include "ZoomWaveform.h"

#include "Theme.h"
#include "WaveformPainter.h"

#include <cmath>

namespace openmixer::ui
{

namespace
{
    constexpr double kWheelZoomStep = 1.25;
    constexpr float kMinGridSpacingPx = 6.0F;   // thinner than this the beat lines turn to noise
    constexpr int kCacheViews = 3;   // strip width in view widths: one visible, one of margin each side
}

ZoomWaveform::ZoomWaveform (DeckBinding binding) : deckRef (binding) {}

void ZoomWaveform::setWaveform (std::shared_ptr<const analysis::WaveformData> data)
{
    waveform = std::move (data);
    invalidateCache();
    repaint();
}

void ZoomWaveform::setFramesPerPixel (double fpp)
{
    framesPerPixel = wave::clampZoom (fpp);
    invalidateCache();
    repaint();
}

void ZoomWaveform::mouseWheelMove (const juce::MouseEvent&, const juce::MouseWheelDetails& wheel)
{
    zoomBy (wheel.deltaY > 0.0F ? 1.0 / kWheelZoomStep : kWheelZoomStep);
}

wave::View ZoomWaveform::currentView() const
{
    return { deckRef.deck().getPositionFrames(), framesPerPixel, static_cast<double> (getWidth()) };
}

ZoomWaveform::Shown ZoomWaveform::snapshot() const
{
    const auto& deck = deckRef.deck();
    Shown now;
    now.position = deck.getPositionFrames();
    const auto loop = deck.getLoop();
    now.loopStart = loop.start;
    now.loopEnd = loop.end;
    now.looping = deck.isLooping();
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        now.cues[static_cast<size_t> (i)] = deck.getHotcue (i).value_or (-1);
    return now;
}

void ZoomWaveform::refresh()
{
    const auto now = snapshot();
    if (now == shown)
        return;
    shown = now;
    ++repaintRequests;
    repaint();
}

void ZoomWaveform::invalidateCache() { cache = {}; }

void ZoomWaveform::renderCache (const wave::View& view)
{
    ++cacheRenders;
    cacheCentre = static_cast<double> (view.centreFrame);
    cacheFramesPerPixel = view.framesPerPixel;
    const int w = std::max (1, getWidth()) * kCacheViews;
    cache = juce::Image (juce::Image::ARGB, w, std::max (1, getHeight()), true);
    juce::Graphics g (cache);
    const double half = static_cast<double> (w) * 0.5 * view.framesPerPixel;
    wave::paintColumns (g, *waveform, juce::Rectangle<float> (0.0F, 2.0F, static_cast<float> (w), static_cast<float> (getHeight()) - 4.0F),
                        frameToSample (cacheCentre - half), frameToSample (cacheCentre + half));
}

// Offset of the strip's left edge inside the view, in pixels.
void ZoomWaveform::paintWaveform (juce::Graphics& g, const wave::View& view)
{
    const auto stale = [&] { return ! cache.isValid() || cache.getHeight() != getHeight() || cache.getWidth() != getWidth() * kCacheViews
                                    || cacheFramesPerPixel != view.framesPerPixel; };
    const auto offsetFor = [&] { return (cacheCentre - static_cast<double> (view.centreFrame)) / view.framesPerPixel - static_cast<double> (getWidth()); };
    if (stale() || std::abs (offsetFor() + static_cast<double> (getWidth())) > static_cast<double> (getWidth()) * 0.5)
        renderCache (view);
    g.drawImageAt (cache, juce::roundToInt (offsetFor()), 0);
}

double ZoomWaveform::frameToSample (double frame) const
{
    const double fileRate = deckRef.deck().getFileSampleRate();
    return waveform != nullptr && fileRate > 0.0 ? frame * waveform->getSampleRate() / fileRate : frame;
}

void ZoomWaveform::paintGrid (juce::Graphics& g, const wave::View& view) const
{
    const auto grid = deckRef.control().getGrid();
    if (grid == nullptr)
        return;
    const int perBar = std::max (1, grid->getBeatsPerBar());
    const double first = std::ceil (grid->beatAtFrame (static_cast<std::int64_t> (view.firstFrame())));
    const double last = grid->beatAtFrame (static_cast<std::int64_t> (view.lastFrame()));
    const auto beatPx = static_cast<float> (grid->frameAtBeat (1.0) - grid->frameAtBeat (0.0)) / static_cast<float> (view.framesPerPixel);
    const bool barsOnly = std::abs (beatPx) < kMinGridSpacingPx;
    for (double beat = first; beat <= last; beat += 1.0)
    {
        const bool isBar = static_cast<long long> (std::llround (beat)) % perBar == 0;
        if (barsOnly && ! isBar)
            continue;
        g.setColour (isBar ? theme::textPrimary().withAlpha (theme::gridBarAlpha()) : theme::textMuted().withAlpha (theme::gridBeatAlpha()));
        g.drawVerticalLine (juce::roundToInt (view.xOfFrame (grid->frameAtBeat (beat))), 0.0F, static_cast<float> (getHeight()));
    }
}

void ZoomWaveform::paintLoop (juce::Graphics& g, const wave::View& view) const
{
    const auto loop = deckRef.deck().getLoop();
    if (! loop.valid())
        return;
    const auto x0 = static_cast<float> (view.xOfFrame (loop.start)), x1 = static_cast<float> (view.xOfFrame (loop.end));
    g.setColour (theme::accentFor (deckRef.index).withAlpha (deckRef.deck().isLooping() ? theme::loopActiveAlpha() : theme::loopIdleAlpha()));
    g.fillRect (juce::Rectangle<float> (x0, 0.0F, x1 - x0, static_cast<float> (getHeight())));
}

void ZoomWaveform::paintCues (juce::Graphics& g, const wave::View& view) const
{
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        if (const auto cue = deckRef.deck().getHotcue (i))
        {
            g.setColour (theme::accentFor (i));
            g.fillRect (static_cast<float> (view.xOfFrame (*cue)) - theme::cueMarkerWidth() * 0.5F, 0.0F, theme::cueMarkerWidth(), static_cast<float> (getHeight()));
        }
}

void ZoomWaveform::paint (juce::Graphics& g)
{
    const auto bounds = getLocalBounds().toFloat();
    g.setColour (theme::well());
    g.fillRoundedRectangle (bounds, theme::controlRadius());
    if (waveform == nullptr || ! waveform->isValid())
    {
        wave::paintEmpty (g, bounds, "No waveform");
        return;
    }
    const auto view = currentView();
    paintLoop (g, view);
    paintWaveform (g, view);
    paintGrid (g, view);
    paintCues (g, view);
    g.setColour (theme::accentFor (deckRef.index));
    g.fillRect (juce::Rectangle<float> (theme::playheadWidth(), bounds.getHeight()).withCentre (bounds.getCentre()));
}

} // namespace openmixer::ui
