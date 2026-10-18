// Settings tab: watch folders (add / remove / rescan, stored in the library database through
// LibraryServices), scan interval and scan-on-start (preferences, read when the library starts).
// `services` may be null (library failed to open): the tab then explains and disables itself.
#pragma once

#include "LibraryServices.h"
#include "SettingsPage.h"

namespace openmixer::ui
{

class LibrarySettingsPage : public SettingsPage, private juce::ListBoxModel
{
public:
    LibrarySettingsPage (Preferences& prefs, LibraryServices* services);
    ~LibrarySettingsPage() override;

    void reflect() override;
    void resized() override;
    juce::Result addFolder (const juce::File& folder);
    juce::Result removeFolder (const juce::String& path);
    void rescan();
    void chooseFolder();   // the "Add..." button: async folder chooser -> addFolder

    const std::vector<juce::String>& getFolders() const noexcept { return folders; }
    juce::ListBox& getList() noexcept { return list; }
    juce::TextButton& getAddButton() noexcept { return addButton; }
    juce::TextButton& getRemoveButton() noexcept { return removeButton; }
    juce::TextButton& getRescanButton() noexcept { return rescanButton; }
    juce::ComboBox& getIntervalBox() noexcept { return intervalBox; }
    juce::ToggleButton& getScanOnStartToggle() noexcept { return scanOnStart; }
    const juce::Label& getStatusLabel() const noexcept { return status; }

private:
    int getNumRows() override { return static_cast<int> (folders.size()); }
    void paintListBoxItem (int row, juce::Graphics& g, int width, int height, bool selected) override;
    void refreshFolders();
    void setStatus (const juce::String& text);

    Preferences& prefs;
    LibraryServices* services;
    std::vector<juce::String> folders;
    juce::ListBox list { "watch folders", this };
    juce::TextButton addButton { "Add..." }, removeButton { "Remove" }, rescanButton { "Rescan now" };
    juce::Component buttonRow;
    juce::ComboBox intervalBox;
    juce::ToggleButton scanOnStart { "Scan watch folders when the app starts" };
    juce::Label status;
    std::unique_ptr<juce::FileChooser> chooser;
};

} // namespace openmixer::ui
