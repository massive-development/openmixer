// One full deck: header, overview + zoomed waveforms, transport, pitch fader, hotcue pads and loop
// controls on a glass card. Drives the engine only through the DeckBinding's router ids.
// Frame ticks come from a VBlankAttachment (~display rate, only while visible); tests call refresh().
// Feeding data: setWaveform() (analysis WaveformData), setTrack() (metadata), setGrid() (BeatGrid).
// Message thread only.
#pragma once

#include "DeckBinding.h"
#include "HotcuePads.h"
#include "LoopControls.h"
#include "OverviewWaveform.h"
#include "PitchFader.h"
#include "TrackDrag.h"
#include "TrackHeader.h"
#include "TransportPanel.h"
#include "ZoomWaveform.h"

#include <juce_gui_basics/juce_gui_basics.h>

namespace openmixer::ui
{

class DeckWidget : public juce::Component, public juce::DragAndDropTarget
{
public:
    explicit DeckWidget (DeckBinding binding);

    void setWaveform (std::shared_ptr<const analysis::WaveformData> data);
    void setTrack (const TrackInfo& info) { header.setTrack (info); }
    void setGrid (std::shared_ptr<const BeatGrid> grid);   // forwards to the engine; the zoom view reads it back

    // A library track dropped on the deck (see TrackDrag.h); the app wires this to the track loader.
    std::function<void (std::int64_t trackId)> onTrackDropped;
    bool isInterestedInDragSource (const SourceDetails& details) override;
    void itemDragEnter (const SourceDetails&) override { setDropHover (true); }
    void itemDragExit (const SourceDetails&) override { setDropHover (false); }
    void itemDropped (const SourceDetails& details) override;

    void refresh();   // one frame tick: pulls state from the deck and repaints what changed

    TrackHeader& getHeader() noexcept { return header; }
    OverviewWaveform& getOverview() noexcept { return overview; }
    ZoomWaveform& getZoom() noexcept { return zoom; }
    TransportPanel& getTransport() noexcept { return transport; }
    PitchFader& getPitch() noexcept { return pitch; }
    HotcuePads& getHotcues() noexcept { return hotcues; }
    LoopControls& getLoops() noexcept { return loops; }

    void paint (juce::Graphics& g) override;
    void resized() override;

private:
    void setDropHover (bool hover);

    bool dropHover = false;
    DeckBinding deckRef;
    TrackHeader header;
    OverviewWaveform overview;
    ZoomWaveform zoom;
    TransportPanel transport;
    PitchFader pitch;
    HotcuePads hotcues;
    LoopControls loops;
    juce::VBlankAttachment frameClock { this, [this] { refresh(); } };
};

} // namespace openmixer::ui
