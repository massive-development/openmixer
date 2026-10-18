// Scrolling zoomed waveform: the playhead stays at the centre while the track moves beneath it.
// Draws beat-grid lines (bars brighter), hotcue markers and the active loop. Wheel zooms.
// Message thread only.
#pragma once

#include "../analysis/WaveformData.h"
#include "DeckBinding.h"
#include "WaveformGeometry.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>

namespace openmixer::ui
{

class ZoomWaveform : public juce::Component
{
public:
    explicit ZoomWaveform (DeckBinding binding);

    void setWaveform (std::shared_ptr<const analysis::WaveformData> data);
    void setFramesPerPixel (double framesPerPixel);   // clamped to wave::kMin/MaxFramesPerPixel
    double getFramesPerPixel() const noexcept { return framesPerPixel; }
    void zoomBy (double factor) { setFramesPerPixel (framesPerPixel * factor); }

    // Frame tick: repaints only when the position, zoom, loop or cues changed.
    void refresh();

    void paint (juce::Graphics& g) override;
    void mouseWheelMove (const juce::MouseEvent&, const juce::MouseWheelDetails& wheel) override;

private:
    wave::View currentView() const;
    void paintGrid (juce::Graphics& g, const wave::View& view) const;
    void paintLoop (juce::Graphics& g, const wave::View& view) const;
    void paintCues (juce::Graphics& g, const wave::View& view) const;
    double frameToSample (double frame) const;

    DeckBinding deckRef;
    std::shared_ptr<const analysis::WaveformData> waveform;
    double framesPerPixel = 256.0;
    juce::int64 shownPosition = -1;
    std::uint64_t shownState = 0;
};

} // namespace openmixer::ui
