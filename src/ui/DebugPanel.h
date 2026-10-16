// Minimal two-deck debug UI (Todo 26a); plain JUCE widgets, replaced by the Wave 5 UI.
#pragma once

#include "../engine/DjEngine.h"
#include "ChannelStripPanel.h"
#include "DeckStrip.h"
#include "MeterBar.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>
#include <vector>

namespace openmixer::ui
{

class DebugPanel : public juce::Component, private juce::Timer
{
public:
    explicit DebugPanel (DjEngine& engine);
    ~DebugPanel() override { stopTimer(); }

    void paint (juce::Graphics& g) override;
    void resized() override;

private:
    void timerCallback() override;
    void addMasterControls();

    DjEngine& engine;
    std::vector<std::unique_ptr<DeckStrip>> decks;
    std::vector<std::unique_ptr<ChannelStripPanel>> channels;
    juce::Slider crossfader { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    juce::Slider master { juce::Slider::LinearVertical, juce::Slider::NoTextBox };
    juce::Slider cueMix { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    juce::Label crossfaderLabel { {}, "Crossfader" }, masterLabel { {}, "Master" }, cueLabel { {}, "Cue / Master" };
    MeterBar masterMeterL, masterMeterR;
};

} // namespace openmixer::ui
