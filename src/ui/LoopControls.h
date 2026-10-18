// Quantized loop controls: eight loop sizes, halve / double, exit and reloop. Sizes route
// "deck.<N>.loop.quantized.<beats>"; exit/reloop route "loop.exit" / "loop.reloop". Message thread only.
#pragma once

#include "DeckBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>

namespace openmixer::ui
{

class LoopControls : public juce::Component
{
public:
    static constexpr int kNumSizes = 8;
    static constexpr std::array<double, kNumSizes> kSizes { 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0 };   // beats

    explicit LoopControls (DeckBinding binding);

    juce::TextButton& sizeButton (int index) noexcept { return sizes[static_cast<size_t> (index)]; }
    juce::TextButton& halve() noexcept { return halveButton; }
    juce::TextButton& twice() noexcept { return doubleButton; }
    juce::TextButton& exitButton() noexcept { return exitLoop; }
    juce::TextButton& reloopButton() noexcept { return reloop; }

    static juce::String loopId (double beats);   // "loop.quantized.0.25"
    double currentLengthBeats() const;           // 0 when no loop has been set or there is no grid
    void refresh();                              // highlights the active size / loop state

    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    void resize (double factor);

    DeckBinding deckRef;
    std::array<juce::TextButton, kNumSizes> sizes;
    juce::TextButton halveButton { "1/2" }, doubleButton { "x2" }, exitLoop { "Exit" }, reloop { "Reloop" };
};

} // namespace openmixer::ui
