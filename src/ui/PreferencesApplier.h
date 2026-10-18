// Pushes preferences into the subsystems that consume them, at start-up and on every change.
// Subsystems sit behind PreferencesTargets so tests use mocks and the app supplies the real glue
// (src/app/AppPreferencesTargets). Only sections that differ from the last apply are pushed, in
// dependency order: plugins (they may provide themes), theme, audio, MIDI. Targets must be idempotent.
// Message thread only.
#pragma once

#include "Preferences.h"

#include <optional>

namespace openmixer::ui
{

class PreferencesTargets
{
public:
    virtual ~PreferencesTargets() = default;
    virtual void applyPluginFolder (const juce::File& folder) = 0;   // load plugins; forwards their themes to the registry
    virtual bool applyTheme (const juce::String& themeId) = 0;       // false if the theme is unknown
    virtual void applyAudio (const AudioPrefs& audio) = 0;
    virtual void applyMidi (const MidiPrefs& midi) = 0;
};

class PreferencesApplier
{
public:
    explicit PreferencesApplier (PreferencesTargets& t) : targets (t) {}

    // Returns how many sections were pushed (everything on the first call).
    int apply (const Preferences& prefs);

private:
    PreferencesTargets& targets;
    std::optional<Preferences> applied;
};

} // namespace openmixer::ui
