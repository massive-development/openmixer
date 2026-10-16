// ParameterTarget that records every change; used by tests and trace tooling.
#pragma once

#include "../core/ParameterTarget.h"

#include <vector>

namespace openmixer
{

struct ParameterChange
{
    juce::String id;
    double value = 0.0;
};

class RecordingTarget : public ParameterTarget
{
public:
    std::vector<ParameterChange> changes;

    bool setParameter (const juce::String& id, double value) override
    {
        changes.push_back ({ id, value });
        return true;
    }

    double last (const juce::String& id, double fallback = 0.0) const
    {
        for (auto it = changes.rbegin(); it != changes.rend(); ++it)
            if (it->id == id)
                return it->value;
        return fallback;
    }

    int count (const juce::String& id) const
    {
        int n = 0;
        for (const auto& c : changes)
            n += c.id == id ? 1 : 0;
        return n;
    }
};

} // namespace openmixer
