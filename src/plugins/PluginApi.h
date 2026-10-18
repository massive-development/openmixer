// Public, versioned plugin API. A plugin is a shared library exporting the C symbols below.
// Build it against the same JUCE version and compiler as the host (C++ types cross the boundary).
#pragma once

#include "../core/ParameterTarget.h"
#include "../engine/ProcessStage.h"

#include <juce_core/juce_core.h>

#include <functional>
#include <memory>

namespace openmixer
{

constexpr int kPluginApiVersion = 1;

struct PluginInfo
{
    juce::String id;        // unique, e.g. "com.example.gain"
    juce::String name;
    juce::String version;
};

enum class StageKind { Fx, PreProcess, PostProcess };

using StageFactory = std::function<std::unique_ptr<ProcessStage>()>;

// Services the host offers a plugin; only valid on the control thread during onLoad/onUnload.
class PluginHost
{
public:
    virtual ~PluginHost() = default;

    // Each returns false if the name/prefix is taken or the data is invalid.
    virtual bool registerStage (const juce::String& stageId, StageKind kind, StageFactory factory) = 0;
    virtual bool registerMidiMap (const juce::String& mapName, const juce::String& json) = 0;
    // The target must outlive the plugin's onUnload(); ids arrive without the prefix.
    virtual bool registerParameterTarget (const juce::String& prefix, ParameterTarget* target) = 0;
    // A UI theme as JSON (schema: src/ui/ThemeJson.h). Added last so older plugins stay ABI compatible;
    // the host validates it when the app registers it with the theme registry.
    virtual bool registerTheme (const juce::String& themeJson) { juce::ignoreUnused (themeJson); return false; }
};

class Plugin
{
public:
    virtual ~Plugin() = default;
    virtual int apiVersion() const noexcept { return kPluginApiVersion; }
    virtual PluginInfo info() const = 0;
    virtual void onLoad (PluginHost& host) = 0;
    virtual void onUnload() {}
};

} // namespace openmixer

#if defined(_WIN32)
 #define OPENMIXER_PLUGIN_EXPORT extern "C" __declspec(dllexport)
#else
 #define OPENMIXER_PLUGIN_EXPORT extern "C" __attribute__((visibility("default")))
#endif

// Exports the entry points the loader looks for (see examples/plugins/GainPlugin.cpp).
#define OPENMIXER_DEFINE_PLUGIN(PluginClass)                                                           \
    OPENMIXER_PLUGIN_EXPORT openmixer::Plugin* openmixer_create_plugin() { return new PluginClass(); } \
    OPENMIXER_PLUGIN_EXPORT void openmixer_destroy_plugin (openmixer::Plugin* p) { delete p; }
