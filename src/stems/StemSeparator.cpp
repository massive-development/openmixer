#include "StemSeparator.h"

namespace openmixer::stems
{

juce::String toString (StemKind kind)
{
    switch (kind)
    {
        case StemKind::Drums:  return "drums";
        case StemKind::Bass:   return "bass";
        case StemKind::Vocals: return "vocals";
        case StemKind::Other:  return "other";
    }
    return "other";
}

bool StemSet::isComplete (size_t expectedLength) const noexcept
{
    if (stems.size() != 4)
        return false;
    for (const auto& entry : stems)
        if (entry.second.size() != expectedLength)
            return false;
    return true;
}

} // namespace openmixer::stems
