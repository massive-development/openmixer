// Read-only view of engine state by string id (the read-side twin of ParameterTarget); feeds controller LEDs.
#pragma once

#include <juce_core/juce_core.h>

namespace openmixer
{

class StateReader
{
public:
    virtual ~StateReader() = default;

    // False if the id is unknown. Lock-free engine reads where the unit offers them; never call from the audio thread.
    virtual bool readState (const juce::String& id, double& out) const = 0;
};

} // namespace openmixer
