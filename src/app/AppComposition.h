// The device-free half of the main window: engine, parameter router, MIDI mapper, plugins, preferences,
// the main view with its mixer and library slots, track loader, deck drop handling and the settings
// overlay. MainComponent adds the audio/MIDI devices around it; the UI gate uses it with a mock device,
// so both exercise the same wiring. Message thread only (except the engine's own audio entry point).
#pragma once

#include "controllers/MidiMapper.h"
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "plugins/PluginRegistry.h"
#include "ui/LibraryBrowser.h"
#include "ui/LibraryServices.h"
#include "ui/MainView.h"
#include "ui/MixerPanel.h"
#include "ui/Preferences.h"
#include "ui/SettingsPanel.h"
#include "ui/ThemeRegistry.h"
#include "ui/TrackLoader.h"

#include <juce_gui_basics/juce_gui_basics.h>

#include <functional>
#include <memory>
#include <optional>

namespace openmixer
{

// What only the device-owning side can provide to the settings overlay.
struct SettingsHooks
{
    ui::AudioSettingsModel* audio = nullptr;          // null hides the Audio tab
    ui::AudioSettingsPage::StatsProvider latency;
    ui::MidiSettingsPage::InputsProvider midiInputs;
    std::function<void()> reloadPlugins;
    std::function<void()> preferencesChanged;         // after the composition saved the preferences
};

class AppComposition
{
public:
    // `host` receives the main view, the settings button and the settings overlay. `libraryOptions` replaces the
    // default (user data dir / OPENMIXER_LIBRARY_DB) options, e.g. for tests.
    AppComposition (juce::Component& host, ui::ThemeRegistry& themes, const juce::File& prefsFile,
                    std::optional<ui::LibraryServices::Options> libraryOptions = std::nullopt);
    ~AppComposition();

    // Builds the settings button; the overlay itself is created on first open.
    void setUpSettings (SettingsHooks hooks);
    void openSettings();
    void closeSettings();
    bool isSettingsOpen() const { return settingsPanel != nullptr && settingsPanel->isVisible(); }

    void layout();   // call from the host's resized()

    DjEngine engine;
    ParameterRouter router;
    MidiMapper mapper;
    PluginRegistry plugins;   // after engine/router: unloaded first
    ui::Preferences prefs;

    ui::MainView& getView() noexcept { return *mainView; }
    ui::MixerPanel& getMixer() noexcept { return *mixer; }
    ui::LibraryBrowser* getBrowser() noexcept { return browser; }
    ui::LibraryServices* getServices() noexcept { return libraryServices.get(); }
    ui::TrackLoader* getLoader() noexcept { return trackLoader.get(); }
    ui::SettingsPanel* getSettingsPanel() noexcept { return settingsPanel.get(); }
    juce::TextButton& getSettingsButton() noexcept { return settingsButton; }
    const juce::File& getPrefsFile() const noexcept { return prefsFile; }
    bool hasLibrary() const noexcept { return browser != nullptr; }

private:
    void setUpRouter();
    void buildContent();
    void buildLibrary (std::optional<ui::LibraryServices::Options> overrideOptions);
    void wireDeckDrops();
    void placeSettingsButton();
    void preferencesChanged();

    juce::Component& host;
    ui::ThemeRegistry& themes;
    juce::File prefsFile;
    SettingsHooks hooks;
    // Declaration order is start-up order (destroyed in reverse): services, view, then the loader that points at both.
    std::unique_ptr<ui::LibraryServices> libraryServices;
    std::unique_ptr<ui::MainView> mainView;
    std::unique_ptr<ui::TrackLoader> trackLoader;
    ui::MixerPanel* mixer = nullptr;
    ui::LibraryBrowser* browser = nullptr;
    juce::TextButton settingsButton { "Settings" };
    std::unique_ptr<ui::SettingsPanel> settingsPanel;   // built on first open; before services in destruction
};

} // namespace openmixer
