// Field registry for search: maps a query field name ("bpm", "artist") to how it filters and sorts.
// Extension point: plugins add fields with a Custom compile hook.
#pragma once

#include "SearchQuery.h"

#include <functional>
#include <vector>

namespace openmixer::library
{

enum class FieldKind { Text, Number, Date, Custom };

struct SearchField
{
    juce::String name;                   // lower-case token used in query strings
    FieldKind kind = FieldKind::Text;
    std::string column;                  // code-constant SQL expression, e.g. "tracks.bpm" (not for Custom)
    std::vector<std::string> sortColumns;   // code-constant expressions; empty = not sortable
    // Custom only: append SQL for `term` (values via fragment.args). Never splice user text into sql.
    std::function<juce::Result (const FilterTerm& term, SqlFragment& out)> compile;
};

class SearchFieldRegistry
{
public:
    // Built-ins: title artist album genre comment path year bpm rating plays duration added played key playlist.
    static SearchFieldRegistry withDefaults();

    // Replaces a field of the same name.
    void add (SearchField field);
    const SearchField* find (const juce::String& name) const;
    juce::StringArray names() const;

private:
    std::vector<SearchField> fields;
};

// The text columns that free-text terms search.
const std::vector<std::string>& freeTextColumns();

} // namespace openmixer::library
