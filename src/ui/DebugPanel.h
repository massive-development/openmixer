// Main console (Todo 26a, restyled): deck cards on top, channel strips around the master
// section, crossfader + cue bar at the bottom, all on a glass backdrop. Message thread only.
#pragma once

#include "../engine/DjEngine.h"
#include "ChannelStripPanel.h"
#include "ControlCaption.h"
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

    // 1 = solid backdrop; below 1 the desktop shows through a semi-transparent window.
    void setBackdropOpacity (float opacity);

    void paint (juce::Graphics& g) override;
    void resized() override;

private:
    void timerCallback() override;
    void addMasterControls();
    void layoutMixerRow (juce::Rectangle<int> row);
    void layoutMaster (juce::Rectangle<int> column);
    void layoutBottom (juce::Rectangle<int> row);

    DjEngine& engine;
    std::vector<std::unique_ptr<DeckStrip>> decks;
    std::vector<std::unique_ptr<ChannelStripPanel>> channels;
    juce::Slider crossfader { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    juce::Slider master { juce::Slider::LinearVertical, juce::Slider::NoTextBox };
    juce::Slider cueMix { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    juce::Slider cueLevel { juce::Slider::LinearHorizontal, juce::Slider::NoTextBox };
    juce::Label crossfaderLabel, masterLabel, cueLabel, cueLevelLabel, meterLabelL, meterLabelR;
    MeterBar masterMeterL, masterMeterR;
    juce::Rectangle<int> headerArea, masterCard, bottomCard;
    float backdropOpacity = 1.0F;
};

} // namespace openmixer::ui
