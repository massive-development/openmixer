// The mixer section for the main window's "mixer" slot: channel strips around the master section,
// crossfader and headphone cue below, and an effects tab. All controls route "mixer.*" / "fx.*"
// ids through the binding's target. A VBlankAttachment drives refresh() while visible (tests call
// it directly). Message thread only.
#pragma once

#include "ChannelStripWidget.h"
#include "CrossfaderSection.h"
#include "CueSection.h"
#include "FxPanel.h"
#include "MasterSection.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <array>
#include <memory>
#include <vector>

namespace openmixer::ui
{

class MixerPanel : public juce::Component
{
public:
    enum class Page { Mixer, Fx };

    explicit MixerPanel (MixerBinding binding);

    void setPage (Page p);
    Page getPage() const noexcept { return page; }

    // One frame: mirrors engine state and advances the meters by dtSeconds.
    void refresh (float dtSeconds);

    int getNumChannels() const noexcept { return static_cast<int> (strips.size()); }
    ChannelStripWidget& getChannel (int ch) noexcept { return *strips[static_cast<size_t> (ch)]; }
    MasterSection& getMaster() noexcept { return master; }
    CrossfaderSection& getCrossfader() noexcept { return crossfader; }
    CueSection& getCue() noexcept { return cue; }
    FxPanel& getFx() noexcept { return fxPanel; }
    juce::TextButton& getPageButton (Page p) noexcept { return pageButtons[static_cast<size_t> (p)]; }

    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    void layoutStrips (juce::Rectangle<int> row);
    void tick();

    MixerBinding binding;
    Page page = Page::Mixer;
    std::vector<std::unique_ptr<ChannelStripWidget>> strips;
    MasterSection master;
    CrossfaderSection crossfader;
    CueSection cue;
    FxPanel fxPanel;
    std::array<juce::TextButton, 2> pageButtons;
    double lastTickMs = 0.0;
    juce::VBlankAttachment frameClock { this, [this] { tick(); } };
};

} // namespace openmixer::ui
