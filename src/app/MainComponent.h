// Main window content: audio device glue, engine, MIDI, and the main view (decks + mixer/library slots).
// The library slot is the browser; tracks load into decks through the TrackLoader (drag, double-click, Load buttons).
#pragma once

#include <JuceHeader.h>

#include "AppComposition.h"
#include "AppPreferencesTargets.h"
#include "controllers/MidiInputRouter.h"
#include "engine/LatencyMonitor.h"
#include "engine/OutputRouter.h"
#include "ui/JuceAudioSettingsModel.h"
#include "ui/PreferencesApplier.h"
#include "ui/ThemeRegistry.h"

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
    void openSettings() { composition.openSettings(); }
    void closeSettings() { composition.closeSettings(); }

private:
    void timerCallback() override;   // control tick: sync, FX tempo, slip
    void preferencesChanged();       // a settings page edited the preferences (already saved): apply
    openmixer::AudioDeviceControl audioControl();
    openmixer::LatencyStats latencyStats() const { return latency.getStats(); }

    openmixer::ui::ThemeRegistry& themes;
    openmixer::ui::JuceAudioSettingsModel audioModel { deviceManager };   // before composition: its settings panel uses it and runs a timer, so it dies first
    openmixer::AppComposition composition;   // engine, router, mapper, plugins, preferences, view, library, settings
    openmixer::MidiInputRouter midiInput { deviceManager, composition.mapper };
    openmixer::OutputRouter outputs;
    openmixer::LatencyMonitor latency;   // written on the audio thread, read by the settings page
    openmixer::AppPreferencesTargets targets { audioControl(), midiInput, composition.mapper, composition.plugins, themes, outputs };
    openmixer::ui::PreferencesApplier applier { targets };
    double lastTickMs = 0.0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (MainComponent)
};
