// Opens a plugin shared library; the plugin is destroyed by the library that created it.
#pragma once

#include "PluginApi.h"

#include <juce_core/juce_core.h>

#include <memory>

namespace openmixer
{

struct PluginDeleter
{
    void (*destroy) (Plugin*) = nullptr;
    void operator() (Plugin* p) const noexcept { if (p != nullptr && destroy != nullptr) destroy (p); }
};

using PluginPtr = std::unique_ptr<Plugin, PluginDeleter>;

// Member order matters: the plugin is destroyed before its library is closed.
struct LoadedPlugin
{
    std::unique_ptr<juce::DynamicLibrary> library;
    PluginPtr plugin;
};

// No allow-list or signature check: any library exporting the entry points loads.
juce::Result loadPluginLibrary (const juce::File& file, LoadedPlugin& out);

} // namespace openmixer
