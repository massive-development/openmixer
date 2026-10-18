#include "SearchCompiler.h"

namespace openmixer::library
{
namespace
{
juce::Result compileFreeText (const FilterTerm& term, SqlFragment& out)
{
    SqlFragment f;
    for (const auto& column : freeTextColumns())
    {
        if (! f.sql.empty())
            f.sql += " OR ";
        f.args.emplace_back (likePattern (term.value));
        f.sql += column + " LIKE ? ESCAPE '\\'";
    }
    out.sql += std::string (term.negated ? "NOT (" : "(") + f.sql + ")";
    out.args.insert (out.args.end(), f.args.begin(), f.args.end());
    return juce::Result::ok();
}

juce::Result compileTerm (const FilterTerm& term, const SearchFieldRegistry& fields, const CompileContext& ctx, SqlFragment& out)
{
    if (term.field.isEmpty())
        return compileFreeText (term, out);

    const auto* field = fields.find (term.field);
    if (field == nullptr)
        return juce::Result::fail ("unknown field '" + term.field + "'");

    switch (field->kind)
    {
        case FieldKind::Text:   return compileTextTerm (*field, term, out);
        case FieldKind::Custom: return field->compile ? field->compile (term, out) : juce::Result::fail (field->name + ": no compiler");
        default:                return compileRangeTerm (*field, term, ctx.nowMs, out);
    }
}

juce::Result compileWhere (const SearchQuery& query, const SearchFieldRegistry& fields, const CompileContext& ctx, SqlFragment& where)
{
    for (const auto& term : query.terms)
    {
        SqlFragment part;
        if (auto r = compileTerm (term, fields, ctx, part); r.failed())
            return r;
        where.sql += (where.sql.empty() ? " WHERE (" : " AND (") + part.sql + ")";
        where.args.insert (where.args.end(), part.args.begin(), part.args.end());
    }
    return juce::Result::ok();
}

juce::Result compileOrder (const SearchQuery& query, const SearchFieldRegistry& fields, std::string& sql)
{
    sql = " ORDER BY ";
    if (query.sortField.isNotEmpty())
    {
        const auto* field = fields.find (query.sortField);
        if (field == nullptr || field->sortColumns.empty())
            return juce::Result::fail ("cannot sort by '" + query.sortField + "'");
        for (const auto& column : field->sortColumns)
            sql += column + (query.descending ? " DESC, " : " ASC, ");
    }
    sql += "tracks.id ASC";   // stable paging
    return juce::Result::ok();
}
} // namespace

juce::Result compileSearch (const SearchQuery& query, const SearchFieldRegistry& fields, const CompileContext& ctx, CompiledSearch& out)
{
    SqlFragment where;
    std::string order;
    if (auto r = compileWhere (query, fields, ctx, where); r.failed())
        return r;
    if (auto r = compileOrder (query, fields, order); r.failed())
        return r;

    out.countSql = "SELECT COUNT(*) FROM tracks" + where.sql;
    out.countArgs = where.args;
    out.selectSql = "SELECT " + std::string (TrackDao::selectColumns()) + " FROM tracks" + where.sql + order + " LIMIT ? OFFSET ?";
    out.selectArgs = where.args;
    out.selectArgs.emplace_back (static_cast<std::int64_t> (query.limit < 0 ? -1 : query.limit));
    out.selectArgs.emplace_back (static_cast<std::int64_t> (std::max (0, query.offset)));
    return juce::Result::ok();
}

} // namespace openmixer::library
