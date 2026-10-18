// Key-filter helper: resolves "8A" / "Am" (and compatible neighbours) to Camelot codes.
#pragma once

#include <juce_core/juce_core.h>

namespace openmixer::library
{

// Empty when `text` is not a key. With `compatible`, includes the keys that mix harmonically
// (same slot, +-1 on the wheel, relative) per analysis::isHarmonicallyCompatible.
juce::StringArray keyCamelotCodes (const juce::String& text, bool compatible);

} // namespace openmixer::library
