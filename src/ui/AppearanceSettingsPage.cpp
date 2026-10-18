#include "AppearanceSettingsPage.h"

namespace openmixer::ui
{

AppearanceSettingsPage::AppearanceSettingsPage (Preferences& p, ThemeRegistry& r) : prefs (p), registry (r)
{
    addRow ("Theme", themeBox);
    addWide (description);
    addNote ("Themes come from the app, plugins and JSON files in your OpenMixer themes folder.");

    themeBox.onChange = [this]
    {
        const int row = themeBox.getSelectedItemIndex();
        if (! filling && row >= 0 && row < static_cast<int> (ids.size()))
            chooseTheme (ids[static_cast<size_t> (row)]);
    };
    registry.addChangeListener (this);
    reflect();
}

AppearanceSettingsPage::~AppearanceSettingsPage() { registry.removeChangeListener (this); }

void AppearanceSettingsPage::reflect()
{
    const juce::ScopedValueSetter<bool> guard (filling, true);
    themeBox.clear (juce::dontSendNotification);
    ids.clear();
    int selected = -1;
    for (const auto& entry : registry.getThemes())
    {
        ids.push_back (entry.info.id);
        themeBox.addItem (entry.info.name.isNotEmpty() ? entry.info.name : entry.info.id, static_cast<int> (ids.size()));
        if (entry.info.id == prefs.theme)
            selected = static_cast<int> (ids.size()) - 1;
    }
    themeBox.setSelectedItemIndex (selected, juce::dontSendNotification);
    const auto* active = registry.find (prefs.theme);
    description.setText (active != nullptr && active->info.author.isNotEmpty() ? "By " + active->info.author : juce::String(),
                         juce::dontSendNotification);
}

void AppearanceSettingsPage::chooseTheme (const juce::String& id)
{
    prefs.theme = id;
    reflect();
    notifyChanged();
}

} // namespace openmixer::ui
