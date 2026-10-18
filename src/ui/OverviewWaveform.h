// Whole-track waveform strip: band-coloured peaks, playhead, hotcue markers; click or drag seeks.
// The waveform is rendered once into an image; per-frame refresh() only repaints the playhead.
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

class OverviewWaveform : public juce::Component
{
public:
    explicit OverviewWaveform (DeckBinding binding);

    void setWaveform (std::shared_ptr<const analysis::WaveformData> data);   // nullptr -> empty state
    std::shared_ptr<const analysis::WaveformData> getWaveform() const noexcept { return waveform; }
    void refresh();   // frame tick: repaints only what changed

    // Seeks to the track fraction under x (also what clicks/drags use). Returns the fraction sent.
    double seekAt (double x);

    void paint (juce::Graphics& g) override;
    void resized() override { cache = {}; }
    void lookAndFeelChanged() override { cache = {}; repaint(); }
    void mouseDown (const juce::MouseEvent& e) override { seekAt (e.position.x); }
    void mouseDrag (const juce::MouseEvent& e) override { seekAt (e.position.x); }

private:
    float playheadX() const;
    void renderCache();
    void paintMarkers (juce::Graphics& g) const;
    juce::Rectangle<int> playheadStrip (float x) const;

    DeckBinding deckRef;
    std::shared_ptr<const analysis::WaveformData> waveform;
    juce::Image cache;
    float shownX = -1.0F;
    std::array<juce::int64, HotcueBank::kNumCues> shownCues {};
};

} // namespace openmixer::ui
