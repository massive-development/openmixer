// Glue between PreferencesApplier and the real subsystems: plugin loading (and forwarding plugin themes
// to the ThemeRegistry), theme selection, audio device + output routing, MIDI inputs and map file.
// Every call is idempotent. Message thread only.
#pragma once

#include "controllers/MidiInputRouter.h"
#include "controllers/MidiMapper.h"
#include "engine/OutputRouter.h"
#include "plugins/PluginRegistry.h"
#include "ui/PreferencesApplier.h"
#include "ui/ThemeRegistry.h"

#include <juce_audio_devices/juce_audio_devices.h>

namespace openmixer
{

class AppPreferencesTargets : public ui::PreferencesTargets
{
public:
    AppPreferencesTargets (juce::AudioDeviceManager& devices, MidiInputRouter& midi, MidiMapper& mapper,
                           PluginRegistry& plugins, ui::ThemeRegistry& themes, OutputRouter& outputs);

    void applyPluginFolder (const juce::File& folder) override;
    bool applyTheme (const juce::String& themeId) override;
    void applyAudio (const ui::AudioPrefs& audio) override;
    void applyMidi (const ui::MidiPrefs& midi) override;

    void reloadPlugins() { applyPluginFolder (pluginFolder); }   // re-scan the folder applied last
    // The state the app asked the device manager for at start-up, so applyAudio does not repeat it.
    void noteAudioRequested (const juce::String& stateXml) { lastAudioState = stateXml; }

private:
    juce::AudioDeviceManager& devices;
    MidiInputRouter& midi;
    MidiMapper& mapper;
    PluginRegistry& plugins;
    ui::ThemeRegistry& themes;
    OutputRouter& outputs;
    juce::File pluginFolder;
    juce::String lastAudioState, lastMapPath;
};

} // namespace openmixer
