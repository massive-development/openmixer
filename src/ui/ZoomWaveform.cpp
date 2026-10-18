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
    constexpr float kPlayheadWidth = 2.0F;
}

ZoomWaveform::ZoomWaveform (DeckBinding binding) : deckRef (binding) {}

void ZoomWaveform::setWaveform (std::shared_ptr<const analysis::WaveformData> data)
{
    waveform = std::move (data);
    repaint();
}

void ZoomWaveform::setFramesPerPixel (double fpp)
{
    framesPerPixel = wave::clampZoom (fpp);
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

// Loop and cue changes fold into one signature so refresh() stays a handful of atomic loads.
void ZoomWaveform::refresh()
{
    const auto pos = deckRef.deck().getPositionFrames();
    const auto loop = deckRef.deck().getLoop();
    std::uint64_t state = static_cast<std::uint64_t> (loop.start) * 31U + static_cast<std::uint64_t> (loop.end) + (deckRef.deck().isLooping() ? 1U : 0U);
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        state = state * 131U + static_cast<std::uint64_t> (deckRef.deck().getHotcue (i).value_or (-1));
    if (pos != shownPosition || state != shownState)
    {
        shownPosition = pos;
        shownState = state;
        repaint();
    }
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
        g.setColour (isBar ? theme::textPrimary().withAlpha (0.55F) : theme::textMuted().withAlpha (0.4F));
        g.drawVerticalLine (juce::roundToInt (view.xOfFrame (grid->frameAtBeat (beat))), 0.0F, static_cast<float> (getHeight()));
    }
}

void ZoomWaveform::paintLoop (juce::Graphics& g, const wave::View& view) const
{
    const auto loop = deckRef.deck().getLoop();
    if (! loop.valid())
        return;
    const auto x0 = static_cast<float> (view.xOfFrame (loop.start)), x1 = static_cast<float> (view.xOfFrame (loop.end));
    g.setColour (theme::accentFor (deckRef.index).withAlpha (deckRef.deck().isLooping() ? 0.25F : 0.08F));
    g.fillRect (juce::Rectangle<float> (x0, 0.0F, x1 - x0, static_cast<float> (getHeight())));
}

void ZoomWaveform::paintCues (juce::Graphics& g, const wave::View& view) const
{
    for (int i = 0; i < HotcueBank::kNumCues; ++i)
        if (const auto cue = deckRef.deck().getHotcue (i))
        {
            g.setColour (theme::accentFor (i));
            g.fillRect (static_cast<float> (view.xOfFrame (*cue)) - 1.0F, 0.0F, 2.0F, static_cast<float> (getHeight()));
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
    wave::paintColumns (g, *waveform, bounds.reduced (0.0F, 2.0F), frameToSample (view.firstFrame()), frameToSample (view.lastFrame()));
    paintGrid (g, view);
    paintCues (g, view);
    g.setColour (theme::accentFor (deckRef.index));
    g.fillRect (juce::Rectangle<float> (kPlayheadWidth, bounds.getHeight()).withCentre (bounds.getCentre()));
}

} // namespace openmixer::ui
