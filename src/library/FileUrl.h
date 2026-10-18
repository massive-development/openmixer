// Turns the file:// URLs found in iTunes and Rekordbox exports into local paths.
#pragma once

#include <juce_core/juce_core.h>

namespace openmixer::library
{

// Handles file:///p, file://localhost/p, drive letters (file:///C:/p), UNC hosts and %XX (UTF-8).
// Empty when the text is not a file URL.
juce::String fileUrlToPath (const juce::String& url);

} // namespace openmixer::library
