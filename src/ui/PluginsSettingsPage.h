// Settings tab: the plugin folder (loaded at start-up and when it changes) and the plugins now loaded.
#pragma once

#include "PathRow.h"
#include "SettingsPage.h"

namespace openmixer::ui
{

class PluginsSettingsPage : public SettingsPage
{
public:
    using LoadedProvider = std::function<juce::StringArray()>;

    // `reload` re-scans the current folder (the app's PreferencesTargets); may be empty.
    PluginsSettingsPage (Preferences& prefs, LoadedProvider loaded, std::function<void()> reload);

    void reflect() override;
    void setFolder (const juce::File& folder);
    void useDefaultFolder();

    PathRow& getFolderRow() noexcept { return folderRow; }
    juce::TextButton& getReloadButton() noexcept { return reloadButton; }
    const juce::Label& getLoadedLabel() const noexcept { return loadedLabel; }

private:
    Preferences& prefs;
    LoadedProvider loaded;
    PathRow folderRow { "Choose the plugin folder", {} };
    juce::TextButton reloadButton { "Reload plugins" };
    juce::Label loadedLabel;
};

} // namespace openmixer::ui
