// SQLite connection (RAII) with prepared statements, savepoint transactions and WAL.
//
// Threading: one writer. A Database must be used by one thread at a time (no internal locking beyond
// SQLite's own); the UI thread reads through its own OpenMode::ReadOnly connection to the same file,
// which WAL allows concurrently with the writer. ":memory:" databases are private to one connection.
#pragma once

#include "Statement.h"

namespace openmixer::library
{

class Database
{
public:
    enum class OpenMode { ReadWrite, ReadOnly };

    // nullptr on failure; the reason goes to error when given. Use ":memory:" for tests.
    static std::unique_ptr<Database> open (const juce::String& path, OpenMode mode = OpenMode::ReadWrite,
                                           juce::String* error = nullptr);
    ~Database();

    // Invalid Statement when the SQL does not compile (see lastError()).
    Statement prepare (const char* sql);
    juce::Result exec (const char* sql);
    // Outcome of a read: fails when the statement did not prepare or stepping hit an error (e.g. BUSY).
    juce::Result status (const Statement& s) const;
    void report (const Statement& s, juce::Result* out) const { if (out != nullptr) *out = status (s); }

    std::int64_t lastInsertRowId() const;
    int changes() const;
    juce::String lastError() const;

    // Savepoint depth, so Transaction objects nest.
    int nextSavepointId() noexcept { return ++savepointCounter; }

private:
    struct Closer { void operator() (sqlite3* handle) const noexcept; };
    struct OpenKey { explicit OpenKey() = default; };   // passkey: only open() constructs

public:
    Database (OpenKey, sqlite3* raw) noexcept;

private:
    juce::Result configure (OpenMode mode, bool inMemory);
    juce::Result enableWal();

    std::unique_ptr<sqlite3, Closer> handle;
    int savepointCounter = 0;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (Database)
};

// Rolls back on destruction unless commit() ran. Nests via savepoints, so DAO calls compose.
class Transaction
{
public:
    explicit Transaction (Database& database);
    ~Transaction();

    juce::Result commit();

private:
    Database& db;
    juce::String name;
    bool finished = false;

    JUCE_DECLARE_NON_COPYABLE (Transaction)
};

} // namespace openmixer::library
