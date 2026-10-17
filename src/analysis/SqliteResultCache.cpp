#include "SqliteResultCache.h"

#include <sqlite3.h>

namespace openmixer::analysis
{
namespace
{
struct Finalizer { void operator() (sqlite3_stmt* s) const noexcept { sqlite3_finalize (s); } };
using Statement = std::unique_ptr<sqlite3_stmt, Finalizer>;

constexpr const char* kSchema =
    "CREATE TABLE IF NOT EXISTS analysis_results ("
    " content_hash TEXT NOT NULL,"
    " analyzer TEXT NOT NULL,"
    " version INTEGER NOT NULL,"
    " payload TEXT NOT NULL,"
    " PRIMARY KEY (content_hash, analyzer));";

Statement prepare (sqlite3* db, const char* sql)
{
    sqlite3_stmt* raw = nullptr;
    if (sqlite3_prepare_v2 (db, sql, -1, &raw, nullptr) != SQLITE_OK)
        return {};
    return Statement (raw);
}

void bindText (sqlite3_stmt* s, int index, const juce::String& text)
{
    sqlite3_bind_text (s, index, text.toRawUTF8(), -1, SQLITE_TRANSIENT);
}

juce::String errorOf (sqlite3* db)
{
    return db != nullptr ? juce::String::fromUTF8 (sqlite3_errmsg (db)) : juce::String ("out of memory");
}
} // namespace

void SqliteResultCache::Closer::operator() (sqlite3* handle) const noexcept
{
    sqlite3_close (handle);
}

std::unique_ptr<SqliteResultCache> SqliteResultCache::open (const juce::String& path, juce::String* error)
{
    sqlite3* raw = nullptr;
    const auto flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX;
    const auto rc = sqlite3_open_v2 (path.toRawUTF8(), &raw, flags, nullptr);
    Handle handle (raw);

    auto result = rc == SQLITE_OK ? createSchema (handle.get()) : juce::Result::fail (errorOf (handle.get()));
    if (result.failed())
    {
        if (error != nullptr)
            *error = result.getErrorMessage();
        return nullptr;
    }
    return std::make_unique<SqliteResultCache> (OpenKey {}, std::move (handle));
}

juce::Result SqliteResultCache::createSchema (sqlite3* handle)
{
    if (sqlite3_exec (handle, kSchema, nullptr, nullptr, nullptr) != SQLITE_OK)
        return juce::Result::fail (errorOf (handle));

    const auto pragma = "PRAGMA user_version = " + juce::String (kSchemaVersion) + ";";
    if (sqlite3_exec (handle, pragma.toRawUTF8(), nullptr, nullptr, nullptr) != SQLITE_OK)
        return juce::Result::fail (errorOf (handle));
    return juce::Result::ok();
}

std::optional<juce::String> SqliteResultCache::find (const juce::String& contentHash,
                                                     const juce::String& analyzerId, int version)
{
    const std::lock_guard<std::mutex> lock (mutex);
    auto s = prepare (db.get(), "SELECT payload FROM analysis_results"
                                " WHERE content_hash = ?1 AND analyzer = ?2 AND version = ?3;");
    if (s == nullptr)
        return std::nullopt;

    bindText (s.get(), 1, contentHash);
    bindText (s.get(), 2, analyzerId);
    sqlite3_bind_int (s.get(), 3, version);
    if (sqlite3_step (s.get()) != SQLITE_ROW)
        return std::nullopt;

    const auto* text = reinterpret_cast<const char*> (sqlite3_column_text (s.get(), 0));
    return juce::String::fromUTF8 (text != nullptr ? text : "");
}

bool SqliteResultCache::store (const juce::String& contentHash, const juce::String& analyzerId, int version,
                               const juce::String& payload)
{
    const std::lock_guard<std::mutex> lock (mutex);
    auto s = prepare (db.get(), "INSERT OR REPLACE INTO analysis_results"
                                " (content_hash, analyzer, version, payload) VALUES (?1, ?2, ?3, ?4);");
    if (s == nullptr)
        return false;

    bindText (s.get(), 1, contentHash);
    bindText (s.get(), 2, analyzerId);
    sqlite3_bind_int (s.get(), 3, version);
    bindText (s.get(), 4, payload);
    return sqlite3_step (s.get()) == SQLITE_DONE;
}

int SqliteResultCache::erase (const juce::String& contentHash)
{
    const std::lock_guard<std::mutex> lock (mutex);
    auto s = prepare (db.get(), "DELETE FROM analysis_results WHERE content_hash = ?1;");
    if (s == nullptr)
        return 0;

    bindText (s.get(), 1, contentHash);
    return sqlite3_step (s.get()) == SQLITE_DONE ? sqlite3_changes (db.get()) : 0;
}

} // namespace openmixer::analysis
