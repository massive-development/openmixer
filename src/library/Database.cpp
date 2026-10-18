#include "Database.h"

#include <sqlite3.h>

namespace openmixer::library
{

void Database::Closer::operator() (sqlite3* h) const noexcept { sqlite3_close (h); }

Database::Database (OpenKey, sqlite3* raw) noexcept : handle (raw) {}
Database::~Database() = default;

std::unique_ptr<Database> Database::open (const juce::String& path, OpenMode mode, juce::String* error)
{
    sqlite3* raw = nullptr;
    const auto access = mode == OpenMode::ReadOnly ? SQLITE_OPEN_READONLY : (SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE);
    const auto rc = sqlite3_open_v2 (path.toRawUTF8(), &raw, access | SQLITE_OPEN_FULLMUTEX, nullptr);
    auto db = std::make_unique<Database> (OpenKey {}, raw);

    auto result = rc == SQLITE_OK ? db->configure (mode, path == ":memory:") : juce::Result::fail (db->lastError());
    if (result.failed())
    {
        if (error != nullptr)
            *error = result.getErrorMessage();
        return nullptr;
    }
    return db;
}

juce::Result Database::configure (OpenMode mode, bool inMemory)
{
    // Cascading deletes (cues, playlist entries) depend on this; it is off by default in SQLite.
    auto result = exec ("PRAGMA foreign_keys = ON;");
    if (result.wasOk() && mode == OpenMode::ReadWrite && ! inMemory)
        result = exec ("PRAGMA journal_mode = WAL;");   // lets the UI read while the writer commits
    return result;
}

Statement Database::prepare (const char* sql)
{
    sqlite3_stmt* raw = nullptr;
    if (sqlite3_prepare_v2 (handle.get(), sql, -1, &raw, nullptr) != SQLITE_OK)
        return {};
    return Statement (handle.get(), raw);
}

juce::Result Database::exec (const char* sql)
{
    if (sqlite3_exec (handle.get(), sql, nullptr, nullptr, nullptr) != SQLITE_OK)
        return juce::Result::fail (lastError());
    return juce::Result::ok();
}

std::int64_t Database::lastInsertRowId() const { return sqlite3_last_insert_rowid (handle.get()); }
int Database::changes() const { return sqlite3_changes (handle.get()); }

juce::String Database::lastError() const
{
    return handle != nullptr ? juce::String::fromUTF8 (sqlite3_errmsg (handle.get())) : juce::String ("out of memory");
}

Transaction::Transaction (Database& database)
    : db (database), name ("om_sp" + juce::String (database.nextSavepointId()))
{
    finished = db.exec (("SAVEPOINT " + name + ";").toRawUTF8()).failed();
}

Transaction::~Transaction()
{
    if (finished)
        return;
    db.exec (("ROLLBACK TO " + name + "; RELEASE " + name + ";").toRawUTF8());
}

juce::Result Transaction::commit()
{
    if (finished)
        return juce::Result::fail ("transaction not active");

    finished = true;
    return db.exec (("RELEASE " + name + ";").toRawUTF8());
}

} // namespace openmixer::library
