// Compiles a SearchQuery to parameterised SQL over the tracks table.
#pragma once

#include "SearchFields.h"
#include "TrackDao.h"

namespace openmixer::library
{

struct CompiledSearch
{
    std::string selectSql;                 // ends with LIMIT ? OFFSET ?
    std::string countSql;
    std::vector<SearchValue> selectArgs;
    std::vector<SearchValue> countArgs;
};

struct CompileContext
{
    std::int64_t nowMs = 0;   // for relative dates such as added:7d
};

juce::Result compileSearch (const SearchQuery& query, const SearchFieldRegistry& fields, const CompileContext& ctx,
                            CompiledSearch& out);

// "%text%" with LIKE wildcards in text escaped (pair with ESCAPE '\').
juce::String likePattern (const juce::String& text);

// Appends "<column> LIKE ? ESCAPE ..." (contains) or "<column> = ?" (value starts with '=') to out.
void appendTextPredicate (const std::string& column, const juce::String& value, SqlFragment& out);

// Built-in field compilers (also reusable by plugin fields).
juce::Result compileTextTerm (const SearchField& field, const FilterTerm& term, SqlFragment& out);
juce::Result compileRangeTerm (const SearchField& field, const FilterTerm& term, std::int64_t nowMs, SqlFragment& out);
juce::Result compileKeyTerm (const FilterTerm& term, SqlFragment& out);
juce::Result compilePlaylistTerm (const FilterTerm& term, SqlFragment& out);
juce::Result compileFolderTerm (const FilterTerm& term, SqlFragment& out);   // anchored path prefix + separator

} // namespace openmixer::library
