// Forward-only schema migrations. Later subsystems (search index, history) append their own entries.
#pragma once

#include "Database.h"

#include <vector>

namespace openmixer::library
{

struct Migration
{
    int version;            // strictly increasing, starting at 1
    const char* name;
    const char* sql;        // one or more statements
};

// The library's own migrations, in order.
const std::vector<Migration>& coreMigrations();

// Applies every migration newer than the database's schema_version, each in its own transaction.
// Fails if the list is not strictly increasing or the database is newer than the list (no downgrades).
juce::Result migrate (Database& db, const std::vector<Migration>& migrations);

// 0 for a database that has never been migrated; -1 if the version could not be read.
int currentSchemaVersion (Database& db);

} // namespace openmixer::library
