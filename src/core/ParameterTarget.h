// Single control surface for engine units: MIDI, UI and tests all drive parameters by string id.
#pragma once

#include <juce_core/juce_core.h>

namespace openmixer
{

class ParameterTarget
{
public:
    virtual ~ParameterTarget() = default;

    // Returns false if the id is unknown. Safe to call from any non-audio thread.
    virtual bool setParameter (const juce::String& id, double value) = 0;
};

} // namespace openmixer
