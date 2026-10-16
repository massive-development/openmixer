#include "PluginLoader.h"

namespace openmixer
{

juce::Result loadPluginLibrary (const juce::File& file, LoadedPlugin& out)
{
    auto lib = std::make_unique<juce::DynamicLibrary>();
    if (! lib->open (file.getFullPathName()))
        return juce::Result::fail ("Cannot open " + file.getFileName());

    using CreateFn = Plugin* (*)();
    auto* create = reinterpret_cast<CreateFn> (lib->getFunction ("openmixer_create_plugin"));
    auto* destroy = reinterpret_cast<void (*)(Plugin*)> (lib->getFunction ("openmixer_destroy_plugin"));
    if (create == nullptr || destroy == nullptr)
        return juce::Result::fail (file.getFileName() + " is not an OpenMixer plugin");

    PluginPtr plugin (create(), PluginDeleter { destroy });
    if (plugin == nullptr)
        return juce::Result::fail ("Plugin factory returned null");
    if (plugin->apiVersion() != kPluginApiVersion)
        return juce::Result::fail ("Unsupported plugin API version " + juce::String (plugin->apiVersion()));

    out.plugin = std::move (plugin);
    out.library = std::move (lib);
    return juce::Result::ok();
}

} // namespace openmixer
