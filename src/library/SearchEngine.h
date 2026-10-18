// Runs searches against a library database. Threading: use on the thread that owns the Database
// (give the UI its own read-only connection).
#pragma once

#include "Database.h"
#include "SearchCompiler.h"
#include "SearchParser.h"
#include "TrackRecord.h"

namespace openmixer::library
{

class SearchEngine
{
public:
    explicit SearchEngine (Database& database, SearchFieldRegistry fieldRegistry = SearchFieldRegistry::withDefaults())
        : db (database), fields (std::move (fieldRegistry)) {}

    SearchFieldRegistry& registry() noexcept { return fields; }
    ParsedQuery parse (const juce::String& text) const { return parseQuery (text, fields); }

    // Reads: pass `status` to tell a failed query (bad field, bad value, busy db) from "no matches";
    // on failure the result is empty and status carries the reason.
    std::vector<TrackRecord> search (const SearchQuery& query, juce::Result* status = nullptr);
    std::vector<TrackRecord> search (const juce::String& queryText, juce::Result* status = nullptr);
    int count (const SearchQuery& query, juce::Result* status = nullptr);   // ignores limit/offset; -1 on failure

    // Clock for relative dates; tests inject a fixed one.
    std::function<std::int64_t()> clock = [] { return static_cast<std::int64_t> (juce::Time::currentTimeMillis()); };

private:
    Database& db;
    SearchFieldRegistry fields;
};

} // namespace openmixer::library
