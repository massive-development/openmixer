#include "PluginsSettingsPage.h"

namespace openmixer::ui
{

PluginsSettingsPage::PluginsSettingsPage (Preferences& p, LoadedProvider l, std::function<void()> reload)
    : prefs (p), loaded (std::move (l))
{
    addRow ("Plugin folder", folderRow);
    addWide (reloadButton);
    addRow ("Loaded plugins", loadedLabel, 3);
    addNote ("Plugins can add audio stages, MIDI maps, parameter targets and themes.");

    loadedLabel.setJustificationType (juce::Justification::topLeft);
    folderRow.onPicked = [this] (const juce::File& f) { setFolder (f); };
    folderRow.onDefault = [this] { useDefaultFolder(); };
    reloadButton.onClick = [this, reload = std::move (reload)]
    {
        if (reload)
            reload();
        reflect();
    };
    reflect();
}

void PluginsSettingsPage::reflect()
{
    folderRow.setPath (prefs.effectivePluginFolder().getFullPathName(), prefs.pluginFolder.isEmpty());
    const auto ids = loaded ? loaded() : juce::StringArray();
    loadedLabel.setText (ids.isEmpty() ? juce::String ("None") : ids.joinIntoString ("\n"), juce::dontSendNotification);
}

void PluginsSettingsPage::setFolder (const juce::File& folder)
{
    prefs.pluginFolder = folder.getFullPathName();
    reflect();
    notifyChanged();
}

void PluginsSettingsPage::useDefaultFolder()
{
    prefs.pluginFolder = {};
    reflect();
    notifyChanged();
}

} // namespace openmixer::ui
