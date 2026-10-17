// Tiny example plugin: a gain stage, a parameter target ("example.gain") and a MIDI map.
// Build as a shared library (see CMakeLists.txt) and drop it into a plugin folder.
#include "plugins/PluginApi.h"

#include <atomic>

namespace
{

class GainStage final : public openmixer::ProcessStage
{
public:
    explicit GainStage (const std::atomic<float>& g) : gain (g) {}

    void process (float* left, float* right, int numSamples) noexcept override
    {
        const auto g = gain.load (std::memory_order_relaxed);
        for (int i = 0; i < numSamples; ++i)
        {
            left[i] *= g;
            right[i] *= g;
        }
    }

private:
    const std::atomic<float>& gain;
};

class GainPlugin final : public openmixer::Plugin, private openmixer::ParameterTarget
{
public:
    openmixer::PluginInfo info() const override { return { "org.openmixer.example.gain", "Example Gain", "1.0" }; }

    void onLoad (openmixer::PluginHost& host) override
    {
        host.registerStage ("example.gain", openmixer::StageKind::PostProcess,
                            [this] { return std::make_unique<GainStage> (gain); });
        host.registerParameterTarget ("example", this);
        host.registerMidiMap ("example.gain-knob", R"({ "name": "Example gain", "mappings": [
            { "type": "cc", "number": 1, "target": "example.gain", "min": 0, "max": 1 } ] })");
    }

private:
    bool setParameter (const juce::String& id, double value) override
    {
        if (id != "gain")
            return false;
        gain.store (static_cast<float> (value), std::memory_order_relaxed);
        return true;
    }

    std::atomic<float> gain { 1.0F };
};

} // namespace

OPENMIXER_DEFINE_PLUGIN (GainPlugin)
