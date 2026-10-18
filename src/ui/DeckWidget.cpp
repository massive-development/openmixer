#include "DeckWidget.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
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
    header.setBounds (area.removeFromTop (theme::deckHeaderHeight()));
    overview.setBounds (area.removeFromTop (theme::deckOverviewHeight()));
    area.removeFromTop (gap);
    pitch.setBounds (area.removeFromRight (theme::deckPitchWidth()));
    area.removeFromRight (gap);
    const int row = theme::deckRowHeight();
    loops.setBounds (area.removeFromBottom (row * 2 + gap));   // size row + action row
    area.removeFromBottom (gap);
    hotcues.setBounds (area.removeFromBottom (row));
    area.removeFromBottom (gap);
    transport.setBounds (area.removeFromBottom (theme::deckTransportHeight()));
    area.removeFromBottom (gap);
    zoom.setVisible (area.getHeight() >= theme::deckMinZoomHeight());   // a short deck hides the zoom instead of overlapping
    zoom.setBounds (area.getHeight() > 0 ? area : juce::Rectangle<int>());
}

} // namespace openmixer::ui
