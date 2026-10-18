// Scrolling zoomed waveform: the playhead stays at the centre while the track moves beneath it.
// Draws beat-grid lines (bars brighter), hotcue markers and the active loop. Wheel zooms.
// Message thread only.
#pragma once

#include "../analysis/WaveformData.h"
#include "DeckBinding.h"
#include "WaveformGeometry.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>
#include <memory>

namespace openmixer::ui
{

class ZoomWaveform : public juce::Component
{
public:
    explicit ZoomWaveform (DeckBinding binding);

    void setWaveform (std::shared_ptr<const analysis::WaveformData> data);
    std::shared_ptr<const analysis::WaveformData> getWaveform() const noexcept { return waveform; }
    void setFramesPerPixel (double framesPerPixel);   // clamped to wave::kMin/MaxFramesPerPixel
    double getFramesPerPixel() const noexcept { return framesPerPixel; }
    void zoomBy (double factor) { setFramesPerPixel (framesPerPixel * factor); }

    // Frame tick: repaints only when the position, zoom, loop or cues changed.
    void refresh();
    int getRepaintRequests() const noexcept { return repaintRequests; }   // test hook: how often refresh() asked for a repaint
    int getCacheRenders() const noexcept { return cacheRenders; }          // test hook: how often the waveform strip was re-rendered

    void paint (juce::Graphics& g) override;
    void resized() override { invalidateCache(); }
    void lookAndFeelChanged() override { invalidateCache(); }
    void mouseWheelMove (const juce::MouseEvent&, const juce::MouseWheelDetails& wheel) override;

private:
    // What the last repaint showed; refresh() compares field by field.
    struct Shown
    {
        juce::int64 position = -1, loopStart = -1, loopEnd = -1;
        bool looping = false;
        std::array<juce::int64, HotcueBank::kNumCues> cues {};
        bool operator== (const Shown& o) const noexcept
        {
            return position == o.position && loopStart == o.loopStart && loopEnd == o.loopEnd && looping == o.looping && cues == o.cues;
        }
    };

    wave::View currentView() const;
    Shown snapshot() const;
    void invalidateCache();
    void renderCache (const wave::View& view);
    void paintWaveform (juce::Graphics& g, const wave::View& view);
    void paintGrid (juce::Graphics& g, const wave::View& view) const;
    void paintLoop (juce::Graphics& g, const wave::View& view) const;
    void paintCues (juce::Graphics& g, const wave::View& view) const;
    double frameToSample (double frame) const;

    DeckBinding deckRef;
    std::shared_ptr<const analysis::WaveformData> waveform;
    double framesPerPixel = 256.0;
    Shown shown;
    int repaintRequests = 0, cacheRenders = 0;

    // The waveform is rendered once into a strip three views wide centred on cacheCentre, then blitted
    // at the scroll offset; it is re-rendered only when the view leaves the strip or zoom/size/theme/data change.
    juce::Image cache;
    double cacheCentre = 0.0, cacheFramesPerPixel = 0.0;
};

} // namespace openmixer::ui
