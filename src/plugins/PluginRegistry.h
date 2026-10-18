// Loads plugins and keeps what they register: stages, MIDI maps and parameter targets.
// Control thread only. Mixers and routers it was given must outlive it (or be detached/unbound first).
#pragma once

#include "../controllers/MidiMapper.h"
#include "../controllers/ParameterRouter.h"
#include "PluginLoader.h"
#include "StageCatalog.h"

#include <map>
#include <memory>
#include <vector>

namespace openmixer
{

class PluginRegistry : private PluginHost
{
public:
    PluginRegistry() = default;
    ~PluginRegistry();

    PluginRegistry (const PluginRegistry&) = delete;
    PluginRegistry& operator= (const PluginRegistry&) = delete;

    // Plugins from a shared library, an in-process object (built-ins, tests) or a whole folder.
    juce::Result loadLibrary (const juce::File& file);
    juce::Result addPlugin (std::unique_ptr<Plugin> plugin);
    int loadDirectory (const juce::File& dir, juce::StringArray* errors = nullptr);

    bool unload (const juce::String& pluginId);
    void unloadAll();

    juce::StringArray getPluginIds() const;
    juce::StringArray getStageIds() const { return stages.getIds(); }
    juce::StringArray getMidiMapNames() const;

    // Targets registered by plugins appear under the router now and for later plugins.
    void bindRouter (ParameterRouter* newRouter);

    juce::Result attachStage (const juce::String& stageId, Mixer& mixer, int channel, int slot)
    {
        return stages.attach (stageId, mixer, channel, slot);
    }
    void detachStage (Mixer& mixer, int channel, int slot) { stages.detach (mixer, channel, slot); }

    // Theme JSON registered by plugins; the app forwards these to the UI ThemeRegistry (owner = plugin id).
    struct ProvidedTheme { juce::String id, json, owner; };
    std::vector<ProvidedTheme> getThemes() const;

    juce::Result applyMidiMap (const juce::String& mapName, MidiMapper& mapper) const;

private:
    struct Entry { juce::String id; LoadedPlugin loaded; };
    struct MapEntry { juce::String json, owner; };
    struct TargetEntry { ParameterTarget* target; juce::String owner; };

    bool registerStage (const juce::String& stageId, StageKind kind, StageFactory factory) override;
    bool registerMidiMap (const juce::String& mapName, const juce::String& json) override;
    bool registerParameterTarget (const juce::String& prefix, ParameterTarget* target) override;
    bool registerTheme (const juce::String& themeJson) override;

    juce::Result adopt (LoadedPlugin&& loaded);
    void removeRegistrations (const juce::String& owner);

    std::vector<std::unique_ptr<Entry>> entries;
    Entry* loading = nullptr;
    StageCatalog stages;
    std::map<juce::String, MapEntry> maps;
    std::map<juce::String, TargetEntry> targets;
    std::vector<ProvidedTheme> themes;
    ParameterRouter* router = nullptr;
};

} // namespace openmixer
