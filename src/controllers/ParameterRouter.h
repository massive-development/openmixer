// Routes "prefix.rest" parameter ids to registered targets; the registry is the extension point.
#pragma once

#include "../core/ParameterTarget.h"

#include <atomic>
#include <map>
#include <mutex>

namespace openmixer
{

class ParameterRouter : public ParameterTarget
{
public:
    // Thread-safe between control threads (UI, MIDI). One recursive lock covers lookup AND the target call, so
    // unregisterTarget returns only once no call into that target is in flight (safe to unload its plugin after).
    // Never call from the audio thread. The target must outlive the router (or be unregistered). Re-registering replaces.
    void registerTarget (const juce::String& prefix, ParameterTarget* target);
    void unregisterTarget (const juce::String& prefix);

    bool setParameter (const juce::String& id, double value) override;

    int getUnknownCount() const noexcept { return unknown.load (std::memory_order_relaxed); }

private:
    mutable std::recursive_mutex lock;   // recursive: a target may route a follow-up id back through the router
    std::map<juce::String, ParameterTarget*> targets;
    std::atomic<int> unknown { 0 };
};

} // namespace openmixer
