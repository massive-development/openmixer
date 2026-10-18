// Main window content: audio device glue, engine, MIDI, and the main view (decks + mixer/library slots).
// The library slot is the browser; tracks load into decks through the TrackLoader (drag, double-click, Load buttons).
#pragma once

#include <JuceHeader.h>

#include "AppPreferencesTargets.h"
#include "controllers/MidiInputRouter.h"
#include "controllers/MidiMapper.h"
#include "controllers/ParameterRouter.h"
#include "engine/DjEngine.h"
#include "engine/LatencyMonitor.h"
#include "engine/OutputRouter.h"
#include "plugins/PluginRegistry.h"
#include "ui/JuceAudioSettingsModel.h"
#include "ui/LibraryBrowser.h"
#include "ui/LibraryServices.h"
#include "ui/MainView.h"
#include "ui/MixerPanel.h"
#include "ui/PreferencesApplier.h"
#include "ui/SettingsPanel.h"
#include "ui/ThemeRegistry.h"
#include "ui/TrackLoader.h"

class MainComponent : public juce::AudioAppComponent, public juce::DragAndDropContainer, private juce::Timer
{
public:
    explicit MainComponent (openmixer::ui::ThemeRegistry& themeRegistry);
    ~MainComponent() override;

    void prepareToPlay (int samplesPerBlockExpected, double sampleRate) override;
    void getNextAudioBlock (const juce::AudioSourceChannelInfo& bufferToFill) override;
    void releaseResources() override;

    void resized() override;

    void setBackdropOpacity (float opacity);

    // The settings overlay (gear button, or --settings / OPENMIXER_SETTINGS=1 at start-up).
    void openSettings();
    void closeSettings();

private:
    void setUpMidi();
    void buildContent();
    void buildLibrary();
    void timerCallback() override;   // control tick: sync, FX tempo, slip
    void buildSettings();
    void placeSettingsButton();
    void preferencesChanged();       // a settings page edited the preferences: save + apply
    openmixer::LatencyStats latencyStats() const { return latency.getStats(); }

    openmixer::ui::ThemeRegistry& themes;
    openmixer::ui::Preferences prefs;
    juce::File prefsFile;

    openmixer::DjEngine engine;
    openmixer::ParameterRouter router;
    openmixer::MidiMapper mapper;
    openmixer::MidiInputRouter midiInput { deviceManager, mapper };
    openmixer::PluginRegistry plugins;   // after engine/router: unloaded first
    openmixer::OutputRouter outputs;
    openmixer::LatencyMonitor latency;   // written on the audio thread, read by the settings page
    openmixer::AppPreferencesTargets targets { deviceManager, midiInput, mapper, plugins, themes, outputs };
    openmixer::ui::PreferencesApplier applier { targets };
    openmixer::ui::JuceAudioSettingsModel audioModel { deviceManager };
    // Declaration order is start-up order (destroyed in reverse): services, view, then the loader that points at both.
    std::unique_ptr<openmixer::ui::LibraryServices> libraryServices;
    std::unique_ptr<openmixer::ui::MainView> mainView;
    std::unique_ptr<openmixer::ui::TrackLoader> trackLoader;
    juce::TextButton settingsButton { "Settings" };
    std::unique_ptr<openmixer::ui::SettingsPanel> settingsPanel;   // built on first open; before services in destruction
    double lastTickMs = 0.0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (MainComponent)
};
