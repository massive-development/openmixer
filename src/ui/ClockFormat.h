// One "m:ss" clock formatter for the deck header and the library table.
#pragma once

#include <juce_core/juce_core.h>

#include <algorithm>

namespace openmixer::ui
{

// Tenths truncate (a running clock never runs ahead); whole seconds round (a duration reads as stored).
inline juce::String formatClock (double seconds, bool withTenths)
{
    const double s = std::max (0.0, seconds);
    const int whole = withTenths ? static_cast<int> (s) : juce::roundToInt (s);
    auto text = juce::String (whole / 60) + ":" + juce::String (whole % 60).paddedLeft ('0', 2);
    if (withTenths)
        text << "." << juce::String (static_cast<int> ((s - whole) * 10.0));
    return text;
}

} // namespace openmixer::ui
