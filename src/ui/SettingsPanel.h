// The settings overlay: a glass card over a dimmed window with one tab per SettingsPage (audio, MIDI,
// library, appearance, plugins). Pages are registered by title, so the app or a plugin can add more with
// addPage(). Pages edit the shared Preferences and the panel forwards each edit to `onChanged` (the
// app saves and applies). Click outside the card, Escape or Close dismisses it. Message thread only.
#pragma once

#include "AppearanceSettingsPage.h"
#include "AudioSettingsPage.h"
#include "LibrarySettingsPage.h"
#include "MidiSettingsPage.h"
#include "PluginsSettingsPage.h"

#include <memory>
#include <vector>

namespace openmixer::ui
{

// What the standard pages need from the app; optional parts hide their tab when absent.
struct SettingsServices
{
    Preferences& prefs;
    ThemeRegistry& themes;
    AudioSettingsModel* audio = nullptr;
    AudioSettingsPage::StatsProvider latency;
    MidiSettingsPage::InputsProvider midiInputs;
    LibraryServices* library = nullptr;
    PluginsSettingsPage::LoadedProvider loadedPlugins;
    std::function<void()> reloadPlugins;
};

class SettingsPanel : public juce::Component
{
public:
    explicit SettingsPanel (SettingsServices services);

    std::function<void()> onChanged;   // a page edited the preferences
    std::function<void()> onClose;     // the user dismissed the panel

    void addPage (const juce::String& title, std::unique_ptr<SettingsPage> page);
    void showPage (int index);
    int getNumPages() const noexcept { return static_cast<int> (pages.size()); }
    int getCurrentPage() const noexcept { return current; }
    SettingsPage* getPage (int index) noexcept { return pages[static_cast<size_t> (index)].page.get(); }
    juce::TextButton& getTabButton (int index) noexcept { return *pages[static_cast<size_t> (index)].tab; }
    juce::TextButton& getCloseButton() noexcept { return closeButton; }
    juce::Rectangle<int> getCardBounds() const noexcept { return card; }

    void reflectAll();   // pages re-read the preferences (e.g. after they changed elsewhere)

    void paint (juce::Graphics& g) override;
    void resized() override;
    void mouseDown (const juce::MouseEvent& e) override;
    bool keyPressed (const juce::KeyPress& key) override;
    void lookAndFeelChanged() override { applyTheme(); }

private:
    struct Entry
    {
        std::unique_ptr<juce::TextButton> tab;
        std::unique_ptr<SettingsPage> page;
    };

    void applyTheme();
    void close() { if (onClose) onClose(); }

    juce::Label title { {}, "Settings" };
    juce::TextButton closeButton { "Close" };
    std::vector<Entry> pages;
    int current = -1;
    juce::Rectangle<int> card;
};

// The five standard tabs, in order.
void addStandardPages (SettingsPanel& panel, const SettingsServices& services);

} // namespace openmixer::ui
