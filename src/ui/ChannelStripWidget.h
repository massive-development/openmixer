// One mixer channel as a glass card: trim, filter, 3-band EQ with kill switches, PFL, FX shortcut,
// crossfader assign, channel fader and a stereo peak meter. Every control routes a
// "mixer.ch<N>.<id>" parameter; buttons mirror the engine state in refresh(). Message thread only.
#pragma once

#include "LabeledKnob.h"
#include "MeterBar.h"
#include "MixerBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>
#include <functional>
#include <memory>

namespace openmixer::ui
{

class ChannelStripWidget : public juce::Component
{
public:
    static constexpr int kBands = 3;   // engine order: low, mid, high

    ChannelStripWidget (MixerBinding binding, int channel);

    std::function<void (int channel)> onFxRequested;   // the FX button: show this channel's rack

    // One frame: pulls engine state onto the controls and feeds the meter.
    void refresh (float dtSeconds);

    LabeledKnob& getTrim() noexcept { return *trim; }
    LabeledKnob& getFilter() noexcept { return *filter; }
    LabeledKnob& getEq (int band) noexcept { return *eq[static_cast<size_t> (band)]; }
    juce::TextButton& getKill (int band) noexcept { return kill[static_cast<size_t> (band)]; }
    juce::TextButton& getPfl() noexcept { return pfl; }
    juce::TextButton& getFxButton() noexcept { return fxButton; }
    juce::TextButton& getAssign (CrossfaderAssign a) noexcept { return assign[static_cast<size_t> (a)]; }
    juce::Slider& getFader() noexcept { return fader; }
    StereoMeter& getMeter() noexcept { return meter; }
    int getChannel() const noexcept { return channel; }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    void build();
    void buildEq();
    void buildButtons();
    void pressKill (int band);
    void layoutKnobs (juce::Rectangle<int>& area);
    void layoutButtons (juce::Rectangle<int>& area);
    void send (const juce::String& id, double value) const { binding.send (MixerBinding::channelId (channel, id), value); }
    bool isKilled (int band) const;

    MixerBinding binding;
    int channel;
    juce::Colour accent;
    juce::Label title, faderCaption;
    std::unique_ptr<LabeledKnob> trim, filter;
    std::array<std::unique_ptr<LabeledKnob>, kBands> eq;
    std::array<juce::TextButton, kBands> kill;
    std::array<double, kBands> restoreDb {};   // EQ value to return to when a kill is released
    juce::TextButton pfl { "PFL" }, fxButton { "FX" };
    std::array<juce::TextButton, 3> assign;   // indexed by CrossfaderAssign
    juce::Slider fader { juce::Slider::LinearVertical, juce::Slider::NoTextBox };
    StereoMeter meter;
};

} // namespace openmixer::ui
