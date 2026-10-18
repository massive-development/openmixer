// Effects panel for one rack at a time (master or a channel): pick the unit, switch it on, set the
// wet/dry mix and the effect's own parameters. Parameter knobs are built from the effect's
// descriptors (FxParamSpec), so any registered effect gets controls for free. Routes
// "fx.<master|ch<N>>.fx<unit>.<param>"; the controls mirror the engine in refresh(). Message thread only.
#pragma once

#include "LabeledKnob.h"
#include "MixerBinding.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <memory>
#include <vector>

namespace openmixer::ui
{

class FxPanel : public juce::Component
{
public:
    // Scope 0 is the master rack; scope n (>= 1) is channel n - 1.
    explicit FxPanel (MixerBinding binding);

    void setScope (int scope);
    void showChannel (int channel) { setScope (channel + 1); }
    void setUnit (int unit);
    int getScope() const noexcept { return scope; }
    int getUnit() const noexcept { return scopeUnits[static_cast<size_t> (scope)]; }
    int getNumScopes() const noexcept { return static_cast<int> (scopeButtons.size()); }

    void refresh();

    juce::ComboBox& getUnitBox() noexcept { return unitBox; }
    juce::TextButton& getEnable() noexcept { return enable; }
    juce::TextButton& getScopeButton (int scopeIndex) noexcept { return *scopeButtons[static_cast<size_t> (scopeIndex)]; }
    LabeledKnob& getMix() noexcept { return *mix; }
    int getNumParamKnobs() const noexcept { return static_cast<int> (paramKnobs.size()); }
    LabeledKnob& getParamKnob (int i) noexcept { return *paramKnobs[static_cast<size_t> (i)]; }
    juce::String getBeatText() const { return beatLabel.getText(); }

    void paint (juce::Graphics& g) override;
    void resized() override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    void applyTheme();
    void buildScopeButtons();
    void fillUnitBox();
    void rebuildParamKnobs();
    void addParamKnob (int index);
    fx::FxRack& rack() const;
    fx::FxUnit* unit() const { return rack().getUnit (getUnit()); }
    juce::String scopeName() const;
    juce::String unitId (const juce::String& param) const { return scopeName() + ".fx" + juce::String (getUnit()) + "." + param; }
    void layoutKnobs (juce::Rectangle<int> area);

    MixerBinding binding;
    int scope = 0;
    std::vector<int> scopeUnits;   // remembered unit per scope
    std::vector<std::unique_ptr<juce::TextButton>> scopeButtons;
    juce::ComboBox unitBox;
    juce::TextButton enable { "On" };
    std::unique_ptr<LabeledKnob> mix;
    std::vector<std::unique_ptr<LabeledKnob>> paramKnobs;
    juce::Label beatLabel;
};

} // namespace openmixer::ui
