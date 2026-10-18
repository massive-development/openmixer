#include "FxPanel.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kScopeGroup = 1003;
    constexpr double kWideRange = 8.0;   // ranges at least this wide read as whole numbers

    juce::String capitalised (std::string_view name)
    {
        auto text = juce::String (juce::CharPointer_UTF8 (std::string (name).c_str()));
        return text.substring (0, 1).toUpperCase() + text.substring (1);
    }

    juce::String percent (double v) { return juce::String (juce::roundToInt (v * 100.0)) + "%"; }

    std::function<juce::String (double)> formatFor (const fx::FxParamSpec& s)
    {
        if (static_cast<double> (s.maxValue) - static_cast<double> (s.minValue) >= kWideRange)
            return [] (double v) { return juce::String (juce::roundToInt (v)); };
        return [] (double v) { return juce::String (v, 2); };
    }

    // "120.0 BPM   1 beat = 500 ms"
    juce::String beatText (double bpm)
    {
        return juce::String (bpm, 1) + " BPM    1 beat = " + juce::String (juce::roundToInt (60000.0 / std::max (bpm, 1.0))) + " ms";
    }
}

FxPanel::FxPanel (MixerBinding b) : binding (b)
{
    scopeUnits.assign (static_cast<size_t> (binding.numChannels()) + 1, 0);
    buildScopeButtons();
    enable.setClickingTogglesState (false);
    enable.setTooltip ("Switch this effect on or off");
    enable.onClick = [this]
    {
        if (auto* u = unit())   // a custom rack may have no units
            binding.sendFx (unitId ("enable"), u->isEnabled() ? 0.0 : 1.0);
        refresh();
    };
    unitBox.onChange = [this] { setUnit (unitBox.getSelectedId() - 1); };
    mix = std::make_unique<LabeledKnob> (LabeledKnob::Spec { "Mix", 0.0, 1.0, 1.0, percent });
    mix->onChange = [this] (double v) { binding.sendFx (unitId ("mix"), v); };
    styleCaption (beatLabel, {});
    beatLabel.setJustificationType (juce::Justification::centredLeft);
    for (juce::Component* c : { static_cast<juce::Component*> (&unitBox), static_cast<juce::Component*> (&enable),
                                static_cast<juce::Component*> (mix.get()), static_cast<juce::Component*> (&beatLabel) })
        addAndMakeVisible (c);
    fillUnitBox();
    rebuildParamKnobs();
    applyTheme();
    refresh();
}

void FxPanel::buildScopeButtons()
{
    for (int s = 0; s <= binding.numChannels(); ++s)
    {
        auto button = std::make_unique<juce::TextButton> (s == 0 ? "Master" : "Ch " + juce::String (s));
        button->setRadioGroupId (kScopeGroup, juce::dontSendNotification);
        button->setTooltip (s == 0 ? "Effects on the master bus" : "Effects on this channel");
        button->onClick = [this, s] { setScope (s); };
        addAndMakeVisible (*button);
        scopeButtons.push_back (std::move (button));
    }
    scopeButtons.front()->setToggleState (true, juce::dontSendNotification);
}

fx::FxRack& FxPanel::rack() const
{
    return scope == 0 ? binding.fx().getMasterRack() : binding.fx().getChannelRack (scope - 1);
}

juce::String FxPanel::scopeName() const { return scope == 0 ? juce::String ("master") : "ch" + juce::String (scope - 1); }

void FxPanel::fillUnitBox()
{
    unitBox.clear (juce::dontSendNotification);
    for (int i = 0; i < rack().getNumUnits(); ++i)
        unitBox.addItem (capitalised (rack().getUnit (i)->getEffect().getName()), i + 1);
    unitBox.setSelectedId (getUnit() + 1, juce::dontSendNotification);
}

void FxPanel::setScope (int s)
{
    scope = juce::jlimit (0, getNumScopes() - 1, s);
    scopeButtons[static_cast<size_t> (scope)]->setToggleState (true, juce::dontSendNotification);
    fillUnitBox();
    rebuildParamKnobs();
    refresh();
}

void FxPanel::setUnit (int u)
{
    scopeUnits[static_cast<size_t> (scope)] = juce::jlimit (0, std::max (0, rack().getNumUnits() - 1), u);
    unitBox.setSelectedId (getUnit() + 1, juce::dontSendNotification);
    rebuildParamKnobs();
    refresh();
}

// One knob per parameter descriptor of the selected effect.
void FxPanel::rebuildParamKnobs()
{
    paramKnobs.clear();
    if (unit() != nullptr)
        for (int i = 0; i < unit()->getEffect().getNumParams(); ++i)
            addParamKnob (i);
    resized();
}

void FxPanel::addParamKnob (int index)
{
    const auto& spec = unit()->getEffect().getParamSpec (index);
    auto knob = std::make_unique<LabeledKnob> (LabeledKnob::Spec { capitalised (spec.name), spec.minValue, spec.maxValue,
                                                                  spec.defaultValue, formatFor (spec) });
    const juce::String paramName (juce::CharPointer_UTF8 (std::string (spec.name).c_str()));
    knob->onChange = [this, paramName] (double v) { binding.sendFx (unitId (paramName), v); };
    knob->setAccent (theme::master());
    addAndMakeVisible (*knob);
    paramKnobs.push_back (std::move (knob));
}

void FxPanel::applyTheme()
{
    enable.setColour (juce::TextButton::buttonOnColourId, theme::master());
    for (auto& s : scopeButtons)
        s->setColour (juce::TextButton::buttonOnColourId, theme::master());
    mix->setAccent (theme::master());
    for (auto& k : paramKnobs)
        k->setAccent (theme::master());
    beatLabel.setFont (theme::font (theme::typeBody()));
    beatLabel.setColour (juce::Label::textColourId, theme::textSecondary());
}

void FxPanel::refresh()
{
    auto* u = unit();
    if (u == nullptr)
        return;
    enable.setToggleState (u->isEnabled(), juce::dontSendNotification);
    mix->setValueQuiet (u->getMix());
    for (int i = 0; i < getNumParamKnobs(); ++i)
        paramKnobs[static_cast<size_t> (i)]->setValueQuiet (u->getEffect().getParamTarget (i));
    beatLabel.setText (beatText (u->getTempo()), juce::dontSendNotification);
}

void FxPanel::paint (juce::Graphics& g)
{
    theme::paintGlassCard (g, getLocalBounds().toFloat().reduced (0.5F));
}

void FxPanel::layoutKnobs (juce::Rectangle<int> area)
{
    const int count = getNumParamKnobs() + 1;
    const int cell = std::min (area.getWidth() / count, theme::mixerKnobHeight() * 2);
    mix->setBounds (area.removeFromLeft (cell));
    for (auto& k : paramKnobs)
        k->setBounds (area.removeFromLeft (cell));
}

void FxPanel::resized()
{
    const int h = theme::mixerButtonHeight(), gap = theme::innerGap();
    auto area = getLocalBounds().reduced (theme::pad());
    auto scopes = area.removeFromTop (h);
    const int cell = scopes.getWidth() / std::max (1, getNumScopes());
    for (auto& s : scopeButtons)
        s->setBounds (scopes.removeFromLeft (cell).reduced (1, 0));
    area.removeFromTop (gap);

    auto row = area.removeFromTop (h);
    unitBox.setBounds (row.removeFromLeft (theme::mixerFxBoxWidth()));
    row.removeFromLeft (gap);
    enable.setBounds (row.removeFromLeft (theme::mixerFxEnableWidth()));
    area.removeFromTop (gap);

    layoutKnobs (area.removeFromTop (theme::mixerKnobHeight() * 3 / 2));
    area.removeFromTop (gap);
    beatLabel.setBounds (area.removeFromTop (h));
}

} // namespace openmixer::ui
