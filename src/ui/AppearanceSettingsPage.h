// Settings tab: theme picker over the ThemeRegistry (built-in, plugin and user themes). Choosing a theme
// stores its id; the PreferencesApplier selects it, which restyles every window live.
#pragma once

#include "SettingsPage.h"
#include "ThemeRegistry.h"

namespace openmixer::ui
{

class AppearanceSettingsPage : public SettingsPage, private juce::ChangeListener
{
public:
    AppearanceSettingsPage (Preferences& prefs, ThemeRegistry& registry);
    ~AppearanceSettingsPage() override;

    void reflect() override;
    void chooseTheme (const juce::String& id);   // what picking a combo row does

    juce::ComboBox& getThemeBox() noexcept { return themeBox; }
    int getNumThemes() const noexcept { return static_cast<int> (ids.size()); }

private:
    void changeListenerCallback (juce::ChangeBroadcaster*) override { reflect(); }

    Preferences& prefs;
    ThemeRegistry& registry;
    juce::ComboBox themeBox;
    juce::Label description;
    std::vector<juce::String> ids;   // combo row -> theme id
    bool filling = false;
};

} // namespace openmixer::ui
