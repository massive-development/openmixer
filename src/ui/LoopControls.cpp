#include "LoopControls.h"

#include "Theme.h"

#include <algorithm>
#include <cmath>

namespace openmixer::ui
{

namespace
{
    constexpr double kMatchTolerance = 0.01;   // beats; a loop snapped to the grid is within rounding of its size
    const char* const kSizeNames[LoopControls::kNumSizes] = { "1/4", "1/2", "1", "2", "4", "8", "16", "32" };
}

LoopControls::LoopControls (DeckBinding binding) : deckRef (binding)
{
    for (int i = 0; i < kNumSizes; ++i)
    {
        sizeButton (i).setButtonText (kSizeNames[i]);
        sizeButton (i).onClick = [this, i] { deckRef.send (loopId (kSizes[static_cast<size_t> (i)]), 1.0); refresh(); };
        addAndMakeVisible (sizeButton (i));
    }
    halveButton.onClick = [this] { resize (0.5); };
    doubleButton.onClick = [this] { resize (2.0); };
    exitLoop.onClick = [this] { deckRef.send ("loop.exit", 1.0); refresh(); };
    reloop.onClick = [this] { deckRef.send ("loop.reloop", 1.0); refresh(); };
    for (auto* b : { &halveButton, &doubleButton, &exitLoop, &reloop })
        addAndMakeVisible (*b);
    applyTheme();
}

void LoopControls::applyTheme()
{
    const auto accent = theme::accentFor (deckRef.index);
    for (auto& b : sizes)
        b.setColour (juce::TextButton::buttonOnColourId, accent);
    reloop.setColour (juce::TextButton::buttonOnColourId, accent);
}

juce::String LoopControls::loopId (double beats) { return "loop.quantized." + juce::String (beats); }

double LoopControls::currentLengthBeats() const
{
    const auto grid = deckRef.control().getGrid();
    const auto loop = deckRef.deck().getLoop();
    return grid != nullptr && loop.valid() ? grid->beatAtFrame (loop.end) - grid->beatAtFrame (loop.start) : 0.0;
}

void LoopControls::resize (double factor)
{
    const double beats = currentLengthBeats();
    if (beats <= 0.0)
        return;   // nothing to scale yet: pick a size first
    const double next = std::clamp (beats * factor, kSizes.front(), kSizes.back());
    deckRef.send (loopId (next), 1.0);
    refresh();
}

void LoopControls::refresh()
{
    const bool active = deckRef.deck().isLooping();
    const double beats = currentLengthBeats();
    for (int i = 0; i < kNumSizes; ++i)
        sizeButton (i).setToggleState (active && std::abs (beats - kSizes[static_cast<size_t> (i)]) < kMatchTolerance, juce::dontSendNotification);
    reloop.setToggleState (active, juce::dontSendNotification);
}

void LoopControls::resized()
{
    auto area = getLocalBounds();
    const int gap = theme::innerGap(), rowH = (area.getHeight() - gap) / 2;
    auto top = area.removeFromTop (rowH);
    area.removeFromTop (gap);
    const int w = (top.getWidth() - gap * (kNumSizes - 1)) / kNumSizes;
    for (auto& b : sizes)
    {
        b.setBounds (top.removeFromLeft (w));
        top.removeFromLeft (gap);
    }
    const int cell = (area.getWidth() - gap * 3) / 4;
    for (auto* b : { &halveButton, &doubleButton, &exitLoop, &reloop })
    {
        b->setBounds (area.removeFromLeft (cell));
        area.removeFromLeft (gap);
    }
}

} // namespace openmixer::ui
