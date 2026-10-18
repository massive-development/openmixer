// Theme file parsing (Todo 25a): JSON -> ThemeTokens with validation. Pure; no global state.
// Missing keys inherit the built-in dark theme; a malformed value rejects the whole theme.
//
//   { "id": "my.theme", "name": "My Theme", "author": "me",
//     "colours":      { "backdropTop": "#FF0E1220", ... },          // #RRGGBB or #AARRGGBB
//     "accents":      [ "#38BDF8", ... ],                           // 1-8 colours, one per deck
//     "spacing":      { "gap": 12, "pad": 12, "innerGap": 8 },
//     "radii":        { "card": 14, "control": 8 },
//     "type":         { "small": 11, "caption": 12, "body": 13, "label": 14, "title": 16, "heading": 17, "mono": 15 },
//     "translucency": { "surface": 1.0, "backdrop": 1.0 } }
#pragma once

#include "ThemeTokens.h"

#include <juce_core/juce_core.h>

namespace openmixer::ui
{

struct ThemeInfo
{
    juce::String id, name, author;
};

struct ParsedTheme
{
    juce::Result status = juce::Result::ok();
    ThemeInfo info;
    ThemeTokens tokens = ThemeTokens::builtinDark();
    juce::StringArray warnings;   // unknown keys: ignored, not fatal
};

ParsedTheme parseTheme (const juce::String& json);
ParsedTheme parseThemeFile (const juce::File& file);

// "#RRGGBB" or "#AARRGGBB" only; false leaves `out` untouched.
bool parseColourHex (const juce::String& text, juce::Colour& out);

} // namespace openmixer::ui
