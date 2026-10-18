#include "AppPreferencesTargets.h"

#include "ui/JuceAudioSettingsModel.h"

namespace openmixer
{

AppPreferencesTargets::AppPreferencesTargets (juce::AudioDeviceManager& d, MidiInputRouter& m, MidiMapper& map,
                                              PluginRegistry& p, ui::ThemeRegistry& t, OutputRouter& o)
    : devices (d), midi (m), mapper (map), plugins (p), themes (t), outputs (o)
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
}

bool AppPreferencesTargets::applyTheme (const juce::String& themeId)
{
    if (themes.select (themeId))
        return true;
    juce::Logger::writeToLog ("theme '" + themeId + "' not found, keeping the current one");
    return false;
}

void AppPreferencesTargets::applyAudio (const ui::AudioPrefs& audio)
{
    outputs.setMasterPair (audio.masterPair);
    outputs.setCuePair (audio.cuePair);
    if (audio.deviceState.isEmpty() || audio.deviceState == lastAudioState)
        return;
    lastAudioState = audio.deviceState;
    const auto xml = juce::parseXML (audio.deviceState);
    if (xml == nullptr)
        return;
    if (const auto error = devices.initialise (0, ui::JuceAudioSettingsModel::kMaxOutputs, xml.get(), true); error.isNotEmpty())
        juce::Logger::writeToLog ("audio: " + error);
}

void AppPreferencesTargets::applyMidi (const ui::MidiPrefs& prefs)
{
    midi.disableAll();
    midi.enable (prefs.enabledInputs, prefs.enableAll);
    const auto map = ui::Preferences::defaultMapFile();
    const auto file = prefs.mapFile.isEmpty() ? map : juce::File (prefs.mapFile);
    if (file.getFullPathName() == lastMapPath || ! file.existsAsFile())
        return;
    lastMapPath = file.getFullPathName();
    if (const auto r = mapper.loadFromFile (file); r.failed())
        juce::Logger::writeToLog ("MIDI map " + file.getFileName() + ": " + r.getErrorMessage());
}

} // namespace openmixer
