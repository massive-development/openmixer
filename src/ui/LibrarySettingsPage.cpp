#include "LibrarySettingsPage.h"

#include "Theme.h"

namespace openmixer::ui
{

namespace
{
    constexpr int kIntervals[] = { 1, 5, 15, 30, 60 };   // minutes
    constexpr int kListRows = 5;
}

LibrarySettingsPage::LibrarySettingsPage (Preferences& p, LibraryServices* s) : prefs (p), services (s)
{
    list.setRowHeight (theme::librarySidebarRowHeight());
    list.setColour (juce::ListBox::backgroundColourId, theme::well());
    addRow ("Watch folders", list, kListRows);

    for (auto* b : { &addButton, &removeButton, &rescanButton })
        buttonRow.addAndMakeVisible (*b);
    addWide (buttonRow);
    addWide (status);
    for (const int m : kIntervals)
        intervalBox.addItem (m == 1 ? juce::String ("Every minute") : "Every " + juce::String (m) + " minutes", m);
    addRow ("Rescan interval", intervalBox);
    addWide (scanOnStart);
    addNote ("Scan interval and scan-on-start take effect the next time OpenMixer starts.");

    addButton.onClick = [this] { chooseFolder(); };
    removeButton.onClick = [this]
    {
        const int row = list.getSelectedRow();
        if (row >= 0 && row < static_cast<int> (folders.size()))
            setStatus (removeFolder (folders[static_cast<size_t> (row)]).failed() ? "Could not remove the folder" : "Folder removed");
    };
    rescanButton.onClick = [this] { rescan(); };
    intervalBox.onChange = [this] { prefs.library.scanIntervalMinutes = intervalBox.getSelectedId(); notifyChanged(); };
    scanOnStart.onClick = [this] { prefs.library.scanOnStart = scanOnStart.getToggleState(); notifyChanged(); };

    const bool available = services != nullptr && services->getWatchFolders() != nullptr;
    for (auto* b : { &addButton, &removeButton, &rescanButton })
        b->setEnabled (available);
    if (! available)
        setStatus (services == nullptr ? "The library is not available." : "This library has no watch folders.");
    reflect();
    list.updateContent();
}

LibrarySettingsPage::~LibrarySettingsPage() = default;

void LibrarySettingsPage::resized()
{
    SettingsPage::resized();
    auto area = buttonRow.getLocalBounds();
    const int w = theme::settingsButtonWidth() + theme::innerGap();
    for (auto* b : { &addButton, &removeButton, &rescanButton })
        b->setBounds (area.removeFromLeft (w).withTrimmedRight (theme::innerGap()));
}

void LibrarySettingsPage::reflect()
{
    intervalBox.setSelectedId (prefs.library.scanIntervalMinutes, juce::dontSendNotification);
    if (intervalBox.getSelectedId() == 0)   // a hand-edited interval not in the list: show it anyway
        intervalBox.setText ("Every " + juce::String (prefs.library.scanIntervalMinutes) + " minutes", juce::dontSendNotification);
    scanOnStart.setToggleState (prefs.library.scanOnStart, juce::dontSendNotification);
    refreshFolders();
}

void LibrarySettingsPage::refreshFolders()
{
    folders.clear();
    if (services != nullptr && services->getWatchFolders() != nullptr)
        folders = services->getWatchFolders()->list();
    list.updateContent();
    list.repaint();
}

void LibrarySettingsPage::setStatus (const juce::String& text) { status.setText (text, juce::dontSendNotification); }

juce::Result LibrarySettingsPage::addFolder (const juce::File& folder)
{
    auto* watch = services != nullptr ? services->getWatchFolders() : nullptr;
    if (watch == nullptr)
        return juce::Result::fail ("no watch folders available");
    const auto result = watch->add (folder);
    setStatus (result.wasOk() ? "Added " + folder.getFileName() + "; scanning" : "Could not add: " + result.getErrorMessage());
    refreshFolders();
    services->notifyLibraryChanged();
    return result;
}

juce::Result LibrarySettingsPage::removeFolder (const juce::String& path)
{
    auto* watch = services != nullptr ? services->getWatchFolders() : nullptr;
    if (watch == nullptr)
        return juce::Result::fail ("no watch folders available");
    const auto result = watch->remove (juce::File (path));
    refreshFolders();
    services->notifyLibraryChanged();
    return result;
}

void LibrarySettingsPage::rescan()
{
    if (services != nullptr && services->getWatchFolders() != nullptr)
    {
        services->getWatchFolders()->triggerScan();
        setStatus ("Rescan started");
    }
}

void LibrarySettingsPage::chooseFolder()
{
    chooser = std::make_unique<juce::FileChooser> ("Choose a folder to watch", juce::File(), juce::String());
    chooser->launchAsync (juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectDirectories,
                          [this] (const juce::FileChooser& fc)
                          {
                              if (const auto folder = fc.getResult(); folder != juce::File())
                                  addFolder (folder);
                          });
}

void LibrarySettingsPage::paintListBoxItem (int row, juce::Graphics& g, int width, int height, bool selected)
{
    if (selected)
    {
        g.setColour (theme::surfaceHover());
        g.fillRect (0, 0, width, height);
    }
    if (row >= static_cast<int> (folders.size()))
        return;
    g.setColour (theme::textPrimary());
    g.setFont (theme::font (theme::typeBody()));
    g.drawText (folders[static_cast<size_t> (row)], theme::innerGap(), 0, width - 2 * theme::innerGap(), height,
                juce::Justification::centredLeft, true);
}

} // namespace openmixer::ui
