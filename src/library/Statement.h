// One prepared SQLite statement (RAII). Not thread-safe; use on the thread that owns its Database.
#pragma once

#include <juce_core/juce_core.h>

#include <cstdint>
#include <memory>
#include <optional>

struct sqlite3;
struct sqlite3_stmt;

namespace openmixer::library
{

class Statement
{
public:
    Statement() = default;
    Statement (sqlite3* owner, sqlite3_stmt* raw) noexcept;

    bool valid() const noexcept { return stmt != nullptr; }

    // Parameter indices are 1-based, as in SQLite.
    Statement& bind (int index, std::nullptr_t);
    Statement& bind (int index, int value);
    Statement& bind (int index, std::int64_t value);
    Statement& bind (int index, double value);
    Statement& bind (int index, const juce::String& value);
    template <typename T>
    Statement& bind (int index, const std::optional<T>& value)
    {
        return value.has_value() ? bind (index, *value) : bind (index, nullptr);
    }

    // Binds all values to parameters 1..N in order.
    template <typename... Args>
    Statement& bindAll (const Args&... values)
    {
        int index = 0;
        (bind (++index, values), ...);
        return *this;
    }

    // True while a row is available; false once finished or on error (check lastResult()).
    bool nextRow();
    // Runs a statement that returns no rows to completion.
    juce::Result run();
    juce::Result lastResult() const { return result; }

    std::int64_t int64 (int column) const;
    int integer (int column) const;
    double real (int column) const;
    juce::String text (int column) const;
    bool isNull (int column) const;
    std::optional<std::int64_t> optInt64 (int column) const;
    std::optional<double> optReal (int column) const;

private:
    struct Finalizer { void operator() (sqlite3_stmt* s) const noexcept; };

    sqlite3* db = nullptr;
    std::unique_ptr<sqlite3_stmt, Finalizer> stmt;
    juce::Result result = juce::Result::ok();
};

} // namespace openmixer::library
