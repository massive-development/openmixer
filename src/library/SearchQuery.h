// Value types for the search/filter engine: the parsed query and the SQL fragments fields compile to.
#pragma once

#include <juce_core/juce_core.h>

#include <cstdint>
#include <string>
#include <variant>
#include <vector>

namespace openmixer::library
{

// One term of a query. Empty `field` is free text matched across the text fields.
struct FilterTerm
{
    juce::String field;
    juce::String value;
    bool negated = false;
};

struct SearchQuery
{
    std::vector<FilterTerm> terms;   // all terms must match (AND)
    juce::String sortField;          // empty = by id
    bool descending = false;
    int limit = -1;                  // < 0 = no limit
    int offset = 0;
};

using SearchValue = std::variant<std::int64_t, double, juce::String>;

// SQL text uses only code-constant names and `?` placeholders; user values go in `args` in order.
struct SqlFragment
{
    std::string sql;
    std::vector<SearchValue> args;
};

} // namespace openmixer::library
