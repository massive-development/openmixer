#include "PluginRegistry.h"

#include <algorithm>

namespace openmixer
{

namespace
{
    bool isSharedLibrary (const juce::File& f)
    {
        return f.hasFileExtension ("so;dll;dylib");
    }
}

PluginRegistry::~PluginRegistry()
{
    unloadAll();
}

juce::Result PluginRegistry::loadLibrary (const juce::File& file)
{
    LoadedPlugin loaded;
    if (const auto r = loadPluginLibrary (file, loaded); r.failed())
        return r;
    return adopt (std::move (loaded));
}

juce::Result PluginRegistry::addPlugin (std::unique_ptr<Plugin> plugin)
{
    if (plugin == nullptr || plugin->apiVersion() != kPluginApiVersion)
        return juce::Result::fail ("Missing plugin or unsupported API version");

    LoadedPlugin loaded;
    loaded.plugin = PluginPtr (plugin.release(), PluginDeleter { [] (Plugin* p) { delete p; } });
    return adopt (std::move (loaded));
}

int PluginRegistry::loadDirectory (const juce::File& dir, juce::StringArray* errors)
{
    int count = 0;
    for (const auto& file : dir.findChildFiles (juce::File::findFiles, false, "*", juce::File::FollowSymlinks::no))
    {
        if (! isSharedLibrary (file))
            continue;
        if (const auto r = loadLibrary (file); r.wasOk())
            ++count;
        else if (errors != nullptr)
            errors->add (r.getErrorMessage());
    }
    return count;
}

juce::Result PluginRegistry::adopt (LoadedPlugin&& loaded)
{
    const auto info = loaded.plugin->info();
    const auto exists = std::any_of (entries.begin(), entries.end(), [&] (const auto& e) { return e->id == info.id; });
    if (info.id.isEmpty() || exists)
        return juce::Result::fail ("Plugin id empty or already loaded: " + info.id);

    entries.push_back (std::make_unique<Entry>());
    auto& entry = *entries.back();
    entry.id = info.id;
    entry.loaded = std::move (loaded);

    loading = &entry;
    entry.loaded.plugin->onLoad (*this);
    loading = nullptr;
    return juce::Result::ok();
}

bool PluginRegistry::unload (const juce::String& pluginId)
{
    const auto it = std::find_if (entries.begin(), entries.end(), [&] (const auto& e) { return e->id == pluginId; });
    if (it == entries.end())
        return false;

    removeRegistrations (pluginId);
    (*it)->loaded.plugin->onUnload();
    entries.erase (it);   // plugin first, then its library
    return true;
}

void PluginRegistry::unloadAll()
{
    while (! entries.empty())
        unload (entries.back()->id);
}

void PluginRegistry::removeRegistrations (const juce::String& owner)
{
    stages.removeOwner (owner);
    for (auto it = maps.begin(); it != maps.end();)
        it = it->second.owner == owner ? maps.erase (it) : std::next (it);
    for (auto it = targets.begin(); it != targets.end();)
    {
        if (it->second.owner != owner) { ++it; continue; }
        if (router != nullptr)
            router->unregisterTarget (it->first);
        it = targets.erase (it);
    }
}

juce::StringArray PluginRegistry::getPluginIds() const
{
    juce::StringArray ids;
    for (const auto& e : entries)
        ids.add (e->id);
    return ids;
}

juce::StringArray PluginRegistry::getMidiMapNames() const
{
    juce::StringArray names;
    for (const auto& [name, entry] : maps)
        names.add (name);
    return names;
}

void PluginRegistry::bindRouter (ParameterRouter* newRouter)
{
    if (router != nullptr)
        for (const auto& [prefix, t] : targets)
            router->unregisterTarget (prefix);
    router = newRouter;
    if (router != nullptr)
        for (const auto& [prefix, t] : targets)
            router->registerTarget (prefix, t.target);
}

juce::Result PluginRegistry::applyMidiMap (const juce::String& mapName, MidiMapper& mapper) const
{
    const auto it = maps.find (mapName);
    if (it == maps.end())
        return juce::Result::fail ("Unknown MIDI map " + mapName);
    return mapper.loadFromJson (it->second.json);
}

bool PluginRegistry::registerStage (const juce::String& stageId, StageKind kind, StageFactory factory)
{
    return loading != nullptr && stages.add (loading->id, stageId, kind, std::move (factory));
}

bool PluginRegistry::registerMidiMap (const juce::String& mapName, const juce::String& json)
{
    MidiMap parsed;
    if (loading == nullptr || mapName.isEmpty() || maps.count (mapName) != 0 || MidiMap::parse (json, parsed).failed())
        return false;
    maps[mapName] = { json, loading->id };
    return true;
}

bool PluginRegistry::registerParameterTarget (const juce::String& prefix, ParameterTarget* target)
{
    if (loading == nullptr || target == nullptr || prefix.isEmpty() || targets.count (prefix) != 0)
        return false;
    targets[prefix] = { target, loading->id };
    if (router != nullptr)
        router->registerTarget (prefix, target);
    return true;
}

} // namespace openmixer
