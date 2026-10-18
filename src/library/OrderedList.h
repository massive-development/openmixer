// Dense 0..n-1 "position" column maintenance for rows grouped by a scope column (library-internal).
// Shared by playlist nesting (scope = parent_id) and playlist entries (scope = playlist_id).
#pragma once

#include "Database.h"

namespace openmixer::library
{

class OrderedList
{
public:
    // table and scopeColumn are code constants, never user input.
    OrderedList (Database& database, const char* table, const char* scopeColumn) noexcept
        : db (database), tableName (table), scope (scopeColumn) {}

    int count (std::optional<std::int64_t> scopeId);
    // Clamps to [0, count]; negative means append.
    int insertIndex (std::optional<std::int64_t> scopeId, int requested);
    // Clamps to [0, count-1].
    int moveTarget (std::optional<std::int64_t> scopeId, int requested);

    juce::Result openGap (std::optional<std::int64_t> scopeId, int index);
    juce::Result closeGap (std::optional<std::int64_t> scopeId, int position);
    // Shifts the rows between the old and new index; caller then stores newPos on the moved row.
    juce::Result shiftForMove (std::optional<std::int64_t> scopeId, int oldPos, int newPos);

private:
    juce::Result shift (std::optional<std::int64_t> scopeId, int delta, int from, int to);

    Database& db;
    juce::String tableName;
    juce::String scope;
};

} // namespace openmixer::library
