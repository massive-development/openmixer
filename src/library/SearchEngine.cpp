#include "SearchEngine.h"

namespace openmixer::library
{
namespace
{
void bindValues (Statement& s, const std::vector<SearchValue>& args)
{
    int index = 0;
    for (const auto& arg : args)
        std::visit ([&] (const auto& v) { s.bind (++index, v); }, arg);
}

void setStatus (juce::Result* out, const juce::Result& r)
{
    if (out != nullptr)
        *out = r;
}
} // namespace

std::vector<TrackRecord> SearchEngine::search (const SearchQuery& query, juce::Result* status)
{
    CompiledSearch compiled;
    if (auto r = compileSearch (query, fields, { clock() }, compiled); r.failed())
    {
        setStatus (status, r);
        return {};
    }

    auto s = db.prepare (compiled.selectSql.c_str());
    bindValues (s, compiled.selectArgs);
    std::vector<TrackRecord> rows;
    while (s.nextRow())
        rows.push_back (TrackDao::readRow (s));

    const auto result = db.status (s);
    setStatus (status, result);
    if (result.failed())
        rows.clear();   // never present a partial page as the answer
    return rows;
}

std::vector<TrackRecord> SearchEngine::search (const juce::String& queryText, juce::Result* status)
{
    const auto parsed = parse (queryText);
    if (parsed.errors.size() > 0)
    {
        setStatus (status, juce::Result::fail (parsed.errors.joinIntoString ("; ")));
        return {};
    }
    return search (parsed.query, status);
}

int SearchEngine::count (const SearchQuery& query, juce::Result* status)
{
    CompiledSearch compiled;
    if (auto r = compileSearch (query, fields, { clock() }, compiled); r.failed())
    {
        setStatus (status, r);
        return -1;
    }

    auto s = db.prepare (compiled.countSql.c_str());
    bindValues (s, compiled.countArgs);
    const int n = s.nextRow() ? s.integer (0) : -1;
    juce::Result r = db.status (s);
    setStatus (status, r);
    return r.failed() ? -1 : n;
}

} // namespace openmixer::library
