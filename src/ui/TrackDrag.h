// Drag-and-drop payload for library tracks: the browser table describes a row with describe(), a deck
// widget accepts it via parse(). Plain string so plugins' components can start or accept the same drag.
#pragma once

#include <juce_core/juce_core.h>

#include <cstdint>
#include <optional>

namespace openmixer::ui::trackdrag
{

inline const juce::String& prefix()
{
    static const juce::String p ("openmixer.track:");
    return p;
}

inline juce::var describe (std::int64_t trackId) { return prefix() + juce::String (trackId); }

inline std::optional<std::int64_t> parse (const juce::var& description)
{
    const auto text = description.toString();
    if (! text.startsWith (prefix()) || text.length() == prefix().length())
        return std::nullopt;
    return text.substring (prefix().length()).getLargeIntValue();
}

} // namespace openmixer::ui::trackdrag
