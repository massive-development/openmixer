// User preferences (Todo 29): one plain model, versioned JSON on disk. JSON rather than
// juce::PropertiesFile because the data is nested (audio/midi/library sections, lists) and versioned,
// stays hand-editable, and matches the theme/layout/map files. Unknown or bad fields fall back to
// their default one by one; a file from a newer schema is refused (defaults, file left alone until the
// next save). No UI or subsystem knowledge here: PreferencesApplier pushes values into subsystems.
// Message thread only.
#pragma once

#include <juce_core/juce_core.h>

namespace openmixer::ui
{

struct AudioPrefs
{
    juce::String deviceState;   // AudioDeviceManager XML (type, devices, rate, buffer, channels); empty = system default
    int masterPair = 0;         // device output pair carrying the master mix (pair p = channels 2p, 2p+1)
    int cuePair = 1;            // headphone cue pair; -1 = none
    bool operator== (const AudioPrefs& o) const { return deviceState == o.deviceState && masterPair == o.masterPair && cuePair == o.cuePair; }
};

struct MidiPrefs
{
    bool enableAll = true;
    juce::StringArray enabledInputs;   // device identifiers, used when enableAll is false
    juce::String mapFile;              // empty = <user data>/OpenMixer/controller_map.json
    bool operator== (const MidiPrefs& o) const
    {
        return enableAll == o.enableAll && enabledInputs == o.enabledInputs && mapFile == o.mapFile;
    }
};

struct LibraryPrefs   // read when the library services are created (restart to change)
{
    bool scanOnStart = true;
    int scanIntervalMinutes = 5;
    bool operator== (const LibraryPrefs& o) const { return scanOnStart == o.scanOnStart && scanIntervalMinutes == o.scanIntervalMinutes; }
};

struct Preferences
{
    static constexpr int kVersion = 1;

    juce::String theme = "openmixer.dark";
    AudioPrefs audio;
    MidiPrefs midi;
    LibraryPrefs library;
    juce::String pluginFolder;   // empty = <user data>/OpenMixer/plugins

    static juce::File defaultFile();                // OPENMIXER_PREFS overrides <user data>/OpenMixer/preferences.json
    static juce::File defaultPluginFolder();
    static juce::File defaultMapFile();
    juce::File effectivePluginFolder() const;
    juce::File effectiveMapFile() const;

    juce::String toJson() const;
    // Fails (out untouched) for non-JSON or a schema newer than kVersion.
    static juce::Result fromJson (const juce::String& json, Preferences& out);

    // A missing file is fine (defaults); a corrupt one is kept as "<name>.bad" and reported.
    static juce::Result load (const juce::File& file, Preferences& out);
    juce::Result save (const juce::File& file) const;

    bool operator== (const Preferences& o) const
    {
        return theme == o.theme && audio == o.audio && midi == o.midi && library == o.library && pluginFolder == o.pluginFolder;
    }
};

} // namespace openmixer::ui
