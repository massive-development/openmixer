#include "MidiSettingsPage.h"

#include "Theme.h"

namespace openmixer::ui
{

void MidiSettingsPage::InputList::setRows (const juce::Array<juce::MidiDeviceInfo>& devices, const juce::StringArray& enabled, bool all,
                                           std::function<void (const juce::String&, bool)> onToggle)
{
    toggles.clear();
    for (const auto& d : devices)
    {
        auto* t = toggles.add (new juce::ToggleButton (d.name));
        t->setToggleState (all || enabled.contains (d.identifier), juce::dontSendNotification);
        t->setEnabled (! all);
        t->onClick = [t, id = d.identifier, onToggle] { onToggle (id, t->getToggleState()); };
        addAndMakeVisible (t);
    }
    resized();
}

int MidiSettingsPage::InputList::getContentHeight() const
{
    return toggles.size() * (theme::settingsRowHeight() + theme::innerGap());
}

void MidiSettingsPage::InputList::resized()
{
    auto area = getLocalBounds();
    for (auto* t : toggles)
    {
        t->setBounds (area.removeFromTop (theme::settingsRowHeight()));
        area.removeFromTop (theme::innerGap());
    }
}

MidiSettingsPage::MidiSettingsPage (Preferences& p, InputsProvider i) : prefs (p), inputs (std::move (i))
{
    addWide (useAll_);
    viewport.setViewedComponent (&list, false);
    viewport.setScrollBarsShown (true, false);
    addRow ("Inputs", viewport, 4);
    addRow ("Controller map", mapRow);
    addNote ("Map file: a MIDI map JSON. Changes apply immediately.");

    useAll_.onClick = [this] { setUseAll (useAll_.getToggleState()); };
    mapRow.onPicked = [this] (const juce::File& f) { setMapFile (f); };
    mapRow.onDefault = [this] { useDefaultMap(); };
    reflect();
}

void MidiSettingsPage::reflect()
{
    useAll_.setToggleState (prefs.midi.enableAll, juce::dontSendNotification);
    const auto devices = inputs ? inputs() : juce::Array<juce::MidiDeviceInfo>();
    list.setRows (devices, prefs.midi.enabledInputs, prefs.midi.enableAll,
                  [this] (const juce::String& id, bool on) { setInputEnabled (id, on); });
    list.setSize (std::max (1, viewport.getMaximumVisibleWidth()), std::max (1, list.getContentHeight()));
    mapRow.setPath (prefs.effectiveMapFile().getFullPathName(), prefs.midi.mapFile.isEmpty());
}

void MidiSettingsPage::resized()
{
    SettingsPage::resized();
    list.setSize (std::max (1, viewport.getMaximumVisibleWidth()), std::max (1, list.getContentHeight()));
}

void MidiSettingsPage::setUseAll (bool useAll)
{
    prefs.midi.enableAll = useAll;
    if (! useAll && prefs.midi.enabledInputs.isEmpty() && inputs)   // leaving "all": start from what was on
        for (const auto& d : inputs())
            prefs.midi.enabledInputs.add (d.identifier);
    reflect();
    notifyChanged();
}

void MidiSettingsPage::setInputEnabled (const juce::String& identifier, bool enabled)
{
    if (enabled)
        prefs.midi.enabledInputs.addIfNotAlreadyThere (identifier);
    else
        prefs.midi.enabledInputs.removeString (identifier);
    notifyChanged();
}

void MidiSettingsPage::setMapFile (const juce::File& file)
{
    prefs.midi.mapFile = file.getFullPathName();
    reflect();
    notifyChanged();
}

void MidiSettingsPage::useDefaultMap()
{
    prefs.midi.mapFile = {};
    reflect();
    notifyChanged();
}

} // namespace openmixer::ui
