// Query-string parser: `artist:foo bpm:120-128 key:~8A "deep house" -genre:techno sort:-bpm limit:50`.
#pragma once

#include "SearchFields.h"

namespace openmixer::library
{

struct ParsedQuery
{
    SearchQuery query;
    juce::StringArray errors;   // bad sort/limit/offset values; the rest of the query is still usable
};

// `name:value` is a filter only when `name` is a registered field (so "12:30" stays free text).
// Reserved: sort:<field> (prefix '-' for descending), limit:<n>, offset:<n>. Quotes group words; '-' negates.
ParsedQuery parseQuery (const juce::String& text, const SearchFieldRegistry& fields);

} // namespace openmixer::library
