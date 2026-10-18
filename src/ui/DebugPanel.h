// Developer console (OPENMIXER_DEBUG_PANEL=1): one DeckStrip per deck (file loading, which the
// main window lacks until the library lands) above the real MixerPanel. Message thread only.
#pragma once

#include "../engine/DjEngine.h"
#include "DeckStrip.h"
#include "MixerPanel.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>
#include <vector>

namespace openmixer::ui
{

class DebugPanel : public juce::Component, private juce::Timer
{
public:
    DebugPanel (DjEngine& engine, ParameterTarget& target);
    ~DebugPanel() override { stopTimer(); }

    // 1 = solid backdrop; below 1 the desktop shows through a semi-transparent window.
    void setBackdropOpacity (float opacity);

    MixerPanel& getMixer() noexcept { return mixer; }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override;

private:
    void applyTheme();
    void timerCallback() override;

    DjEngine& engine;
    std::vector<std::unique_ptr<DeckStrip>> decks;
    MixerPanel mixer;
    juce::Rectangle<int> headerArea;
    float backdropOpacity = 1.0F;
};

} // namespace openmixer::ui
