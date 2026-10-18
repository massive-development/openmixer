#include "Statement.h"

#include <sqlite3.h>

namespace openmixer::library
{

void Statement::Finalizer::operator() (sqlite3_stmt* s) const noexcept
{
    sqlite3_finalize (s);
}

Statement::Statement (sqlite3* owner, sqlite3_stmt* raw) noexcept : db (owner), stmt (raw) {}

Statement& Statement::bind (int index, std::nullptr_t)
{
    sqlite3_bind_null (stmt.get(), index);
    return *this;
}

Statement& Statement::bind (int index, int value)
{
    sqlite3_bind_int (stmt.get(), index, value);
    return *this;
}

Statement& Statement::bind (int index, std::int64_t value)
{
    sqlite3_bind_int64 (stmt.get(), index, value);
    return *this;
}

Statement& Statement::bind (int index, double value)
{
    sqlite3_bind_double (stmt.get(), index, value);
    return *this;
}

Statement& Statement::bind (int index, const juce::String& value)
{
    // TRANSIENT: the juce::String may die before the statement runs.
    sqlite3_bind_text (stmt.get(), index, value.toRawUTF8(), -1, SQLITE_TRANSIENT);
    return *this;
}

bool Statement::nextRow()
{
    if (! valid())
        return false;

    const auto rc = sqlite3_step (stmt.get());
    if (rc == SQLITE_ROW)
        return true;

    if (rc != SQLITE_DONE)
        result = juce::Result::fail (juce::String::fromUTF8 (sqlite3_errmsg (db)));
    return false;
}

juce::Result Statement::run()
{
    if (! valid())
        return juce::Result::fail ("statement failed to prepare");

    while (nextRow()) {}   // drain rows of RETURNING-style statements
    return result;
}

std::int64_t Statement::int64 (int column) const { return sqlite3_column_int64 (stmt.get(), column); }
int Statement::integer (int column) const { return sqlite3_column_int (stmt.get(), column); }
double Statement::real (int column) const { return sqlite3_column_double (stmt.get(), column); }
bool Statement::isNull (int column) const { return sqlite3_column_type (stmt.get(), column) == SQLITE_NULL; }

juce::String Statement::text (int column) const
{
    const auto* chars = reinterpret_cast<const char*> (sqlite3_column_text (stmt.get(), column));
    return juce::String::fromUTF8 (chars != nullptr ? chars : "");
}

std::optional<std::int64_t> Statement::optInt64 (int column) const
{
    return isNull (column) ? std::nullopt : std::optional<std::int64_t> (int64 (column));
}

std::optional<double> Statement::optReal (int column) const
{
    return isNull (column) ? std::nullopt : std::optional<double> (real (column));
}

} // namespace openmixer::library
