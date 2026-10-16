// Routes "prefix.rest" parameter ids to registered targets; the registry is the extension point.
#pragma once

#include "../core/ParameterTarget.h"

#include <atomic>
#include <map>

namespace openmixer
{

class ParameterRouter : public ParameterTarget
{
public:
    // The target must outlive the router (or be unregistered). Re-registering replaces.
    void registerTarget (const juce::String& prefix, ParameterTarget* target);
    void unregisterTarget (const juce::String& prefix);

    bool setParameter (const juce::String& id, double value) override;

    int getUnknownCount() const noexcept { return unknown.load (std::memory_order_relaxed); }

private:
    std::map<juce::String, ParameterTarget*> targets;
    std::atomic<int> unknown { 0 };
};

} // namespace openmixer
