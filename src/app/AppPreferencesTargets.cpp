#include "AppPreferencesTargets.h"


namespace openmixer
{

AppPreferencesTargets::AppPreferencesTargets (AudioDeviceControl d, MidiInputRouter& m, MidiMapper& map,
                                              PluginRegistry& p, ui::ThemeRegistry& t, OutputRouter& o)
    : audioDevice (std::move (d)), midi (m), mapper (map), plugins (p), themes (t), outputs (o)
{
}

// Themes a plugin registered live under its id as owner, so unloading drops exactly those.
void AppPreferencesTargets::applyPluginFolder (const juce::File& folder)
{
    pluginFolder = folder;
    themes.removeSource (ui::ThemeRegistry::Source::Plugin);
    plugins.unloadAll();
    juce::StringArray errors;
    const int loaded = plugins.loadDirectory (folder, &errors);
    for (const auto& e : errors)
        juce::Logger::writeToLog ("plugin: " + e);
    for (const auto& t : plugins.getThemes())
        if (const auto r = themes.addJson (t.json, ui::ThemeRegistry::Source::Plugin, t.owner); r.failed())
            juce::Logger::writeToLog ("plugin theme " + t.id + ": " + r.getErrorMessage());
    juce::Logger::writeToLog ("plugins: " + juce::String (loaded) + " loaded from " + folder.getFullPathName());
    if (wantedTheme.isNotEmpty())   // removing the plugin themes dropped the selection to the default
        themes.select (wantedTheme);
}

bool AppPreferencesTargets::applyTheme (const juce::String& themeId)
{
    wantedTheme = themeId;
    if (themes.select (themeId))
        return true;
    juce::Logger::writeToLog ("theme '" + themeId + "' not found, keeping the current one");
    return false;
}

// A device change made in the settings UI already reached the device (its state equals the stored one), and
// start-up opens the device itself (noteAudioRequested): both skip the re-initialise.
void AppPreferencesTargets::applyAudio (const ui::AudioPrefs& audio)
{
    outputs.setMasterPair (audio.masterPair);
    outputs.setCuePair (audio.cuePair);
    if (audio.deviceState.isEmpty() || audio.deviceState == lastAudioState)
        return;
    lastAudioState = audio.deviceState;
    if (audioDevice.currentState && audioDevice.currentState() == audio.deviceState)
        return;
    const auto xml = juce::parseXML (audio.deviceState);
    if (xml == nullptr || ! audioDevice.initialise)
        return;
    if (const auto error = audioDevice.initialise (*xml); error.isNotEmpty())
        juce::Logger::writeToLog ("audio: " + error);
}

// The map is loaded before inputs are (re)enabled so no message meets a half-built or stale map.
void AppPreferencesTargets::applyMidi (const ui::MidiPrefs& prefs)
{
    const auto file = prefs.mapFile.isEmpty() ? ui::Preferences::defaultMapFile() : juce::File (prefs.mapFile);
    if (file.getFullPathName() != lastMapPath && file.existsAsFile())
    {
        lastMapPath = file.getFullPathName();
        if (const auto r = mapper.loadFromFile (file); r.failed())
            juce::Logger::writeToLog ("MIDI map " + file.getFileName() + ": " + r.getErrorMessage());
    }
    midi.disableAll();
    midi.enable (prefs.enabledInputs, prefs.enableAll);
}

} // namespace openmixer
