// ResultCache on SQLite (one table). Use ":memory:" for tests. Statements are serialised by a
// mutex, so any thread may call it.
#pragma once

#include "ResultCache.h"

#include <memory>
#include <mutex>

struct sqlite3;

namespace openmixer::analysis
{

class SqliteResultCache final : public ResultCache
{
public:
    static constexpr int kSchemaVersion = 1;

    // nullptr on failure; the reason goes to error when given.
    static std::unique_ptr<SqliteResultCache> open (const juce::String& path, juce::String* error = nullptr);

private:
    struct Closer { void operator() (sqlite3* handle) const noexcept; };
    struct OpenKey { explicit OpenKey() = default; };   // passkey: only open() constructs

public:
    using Handle = std::unique_ptr<sqlite3, Closer>;
    SqliteResultCache (OpenKey, Handle handle) noexcept : db (std::move (handle)) {}

    std::optional<juce::String> find (const juce::String& contentHash, const juce::String& analyzerId,
                                      int version) override;
    bool store (const juce::String& contentHash, const juce::String& analyzerId, int version,
                const juce::String& payload) override;
    int erase (const juce::String& contentHash) override;

private:
    static juce::Result createSchema (sqlite3* handle);

    std::mutex mutex;
    Handle db;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (SqliteResultCache)
};

} // namespace openmixer::analysis
