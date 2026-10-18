#include "DeckWidget.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kHeaderHeight = 52, kOverviewHeight = 40, kTransportHeight = 32, kControlRowHeight = 72;
    constexpr int kPitchWidth = 84, kMinZoomHeight = 60;
}

DeckWidget::DeckWidget (DeckBinding binding)
    : deckRef (binding), header (binding), overview (binding), zoom (binding), transport (binding),
      pitch (binding), hotcues (binding), loops (binding)
{
    for (auto* c : std::initializer_list<juce::Component*> { &header, &overview, &zoom, &transport, &pitch, &hotcues, &loops })
        addAndMakeVisible (*c);
}

void DeckWidget::setWaveform (std::shared_ptr<const analysis::WaveformData> data)
{
    overview.setWaveform (data);
    zoom.setWaveform (std::move (data));
}

void DeckWidget::setGrid (std::shared_ptr<const BeatGrid> grid)
{
    deckRef.engine.setGrid (deckRef.index, std::move (grid));
    zoom.repaint();
}

void DeckWidget::refresh()
{
    header.refresh();
    overview.refresh();
    zoom.refresh();
    transport.refresh();
    pitch.refresh();
    hotcues.refresh();
    loops.refresh();
}

void DeckWidget::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F), theme::accentFor (deckRef.index));
}

void DeckWidget::resized()
{
    auto area = getLocalBounds().reduced (theme::pad());
    const int gap = theme::innerGap();
    header.setBounds (area.removeFromTop (kHeaderHeight));
    overview.setBounds (area.removeFromTop (kOverviewHeight));
    area.removeFromTop (gap);
    pitch.setBounds (area.removeFromRight (kPitchWidth));
    area.removeFromRight (gap);
    auto controls = area.removeFromBottom (kControlRowHeight);
    area.removeFromBottom (gap);
    transport.setBounds (area.removeFromBottom (kTransportHeight));
    area.removeFromBottom (gap);
    zoom.setBounds (area.withHeight (std::max (area.getHeight(), kMinZoomHeight)));
    hotcues.setBounds (controls.removeFromLeft (controls.getWidth() / 2 - gap / 2));
    controls.removeFromLeft (gap);
    loops.setBounds (controls);
}

} // namespace openmixer::ui
