// Per-term SQL for the built-in field kinds. Column names come from the registry (code constants);
// every user value is a bound argument.
#include "SearchCompiler.h"
#include "SearchKeys.h"

namespace openmixer::library
{
juce::String likePattern (const juce::String& text)
{
    return "%" + text.replace ("\\", "\\\\").replace ("%", "\\%").replace ("_", "\\_") + "%";
}

namespace
{
constexpr std::int64_t kDayMs = 86400000;

std::string wrapNegation (const std::string& predicate, bool negated)
{
    return negated ? "NOT COALESCE(" + predicate + ", 0)" : predicate;
}

struct Endpoint { double start = 0, end = 0; bool closedEnd = true; };

std::optional<double> parseNumber (const juce::String& s)
{
    if (s.isEmpty() || ! s.containsOnly ("0123456789.") || s.indexOfChar ('.') != s.lastIndexOfChar ('.')
        || s == "." || s.length() > 15)
        return std::nullopt;
    return s.getDoubleValue();
}

std::optional<Endpoint> parseDate (const juce::String& s)
{
    juce::StringArray parts;
    parts.addTokens (s, "-", "");
    if (parts.size() != 3 || parts[0].length() != 4 || ! s.containsOnly ("0123456789-"))
        return std::nullopt;
    const int y = parts[0].getIntValue(), m = parts[1].getIntValue(), d = parts[2].getIntValue();
    if (m < 1 || m > 12 || d < 1 || d > 31)
        return std::nullopt;
    const auto start = juce::Time (y, m - 1, d, 0, 0, 0, 0, false).toMilliseconds();
    return Endpoint { static_cast<double> (start), static_cast<double> (start + kDayMs), false };
}

std::optional<Endpoint> parseEndpoint (FieldKind kind, const juce::String& s)
{
    if (kind == FieldKind::Date)
        return parseDate (s);
    if (auto n = parseNumber (s))
        return Endpoint { *n, *n, true };
    return std::nullopt;
}

struct Interval
{
    std::optional<double> lo, hi;
    const char* loOp = ">=";
    const char* hiOp = "<=";
};

const char* hiOpFor (const Endpoint& e) { return e.closedEnd ? "<=" : "<"; }

std::optional<Interval> intervalFromRange (FieldKind kind, const juce::String& a, const juce::String& b)
{
    auto lo = parseEndpoint (kind, a), hi = parseEndpoint (kind, b);
    if (! lo || ! hi)
        return std::nullopt;
    return Interval { lo->start, hi->end, ">=", hiOpFor (*hi) };
}

std::optional<Interval> intervalFromComparison (FieldKind kind, const juce::String& op, const juce::String& v)
{
    auto e = parseEndpoint (kind, v);
    if (! e)
        return std::nullopt;
    if (op == ">")  return Interval { e->end, std::nullopt, e->closedEnd ? ">" : ">=", "<=" };
    if (op == ">=") return Interval { e->start, std::nullopt, ">=", "<=" };
    if (op == "<")  return Interval { std::nullopt, e->start, ">=", "<" };
    if (op == "<=") return Interval { std::nullopt, e->end, ">=", hiOpFor (*e) };
    return Interval { e->start, e->end, ">=", hiOpFor (*e) };   // "=" or bare value
}

std::optional<Interval> intervalFromRelative (const juce::String& v, std::int64_t nowMs)
{
    const auto digits = v.dropLastCharacters (1);
    if (! v.endsWithChar ('d') || digits.isEmpty() || ! digits.containsOnly ("0123456789") || digits.length() > 6)
        return std::nullopt;
    return Interval { static_cast<double> (nowMs - digits.getLargeIntValue() * kDayMs), std::nullopt, ">=", "<=" };
}

std::optional<Interval> parseInterval (FieldKind kind, const juce::String& v, std::int64_t nowMs)
{
    if (kind == FieldKind::Date)
        if (auto rel = intervalFromRelative (v, nowMs))
            return rel;
    if (const auto dots = v.indexOf (".."); dots > 0)
        return intervalFromRange (kind, v.substring (0, dots), v.substring (dots + 2));
    if (kind == FieldKind::Number)
        if (const auto dash = v.indexOfChar ('-'); dash > 0)
            return intervalFromRange (kind, v.substring (0, dash), v.substring (dash + 1));

    juce::String op;
    for (const char* candidate : { ">=", "<=", ">", "<", "=" })
        if (op.isEmpty() && v.startsWith (candidate))
            op = candidate;
    return intervalFromComparison (kind, op, v.substring (op.length()));
}

SearchValue boundValue (FieldKind kind, double v)
{
    return kind == FieldKind::Date ? SearchValue (static_cast<std::int64_t> (v)) : SearchValue (v);
}
} // namespace

void appendTextPredicate (const std::string& column, const juce::String& value, SqlFragment& out)
{
    if (value.startsWithChar ('='))
    {
        out.sql += column + " = ? COLLATE NOCASE";
        out.args.emplace_back (value.substring (1));
        return;
    }
    out.sql += column + " LIKE ? ESCAPE '\\'";
    out.args.emplace_back (likePattern (value));
}

juce::Result compileTextTerm (const SearchField& field, const FilterTerm& term, SqlFragment& out)
{
    SqlFragment f;
    appendTextPredicate (field.column, term.value, f);
    out.sql += term.negated ? "NOT (" + f.sql + ")" : f.sql;
    out.args.insert (out.args.end(), f.args.begin(), f.args.end());
    return juce::Result::ok();
}

juce::Result compileRangeTerm (const SearchField& field, const FilterTerm& term, std::int64_t nowMs, SqlFragment& out)
{
    const auto iv = parseInterval (field.kind, term.value, nowMs);
    if (! iv)
        return juce::Result::fail (field.name + ": cannot read '" + term.value + "'");

    SqlFragment f;
    if (iv->lo) { f.sql += field.column + " " + iv->loOp + " ?"; f.args.push_back (boundValue (field.kind, *iv->lo)); }
    if (iv->lo && iv->hi) f.sql += " AND ";
    if (iv->hi) { f.sql += field.column + " " + iv->hiOp + " ?"; f.args.push_back (boundValue (field.kind, *iv->hi)); }
    out.sql += "(" + wrapNegation (f.sql, term.negated) + ")";
    out.args.insert (out.args.end(), f.args.begin(), f.args.end());
    return juce::Result::ok();
}

juce::Result compileKeyTerm (const FilterTerm& term, SqlFragment& out)
{
    const bool compatible = term.value.startsWithChar ('~');
    const auto codes = keyCamelotCodes (term.value.substring (compatible ? 1 : 0), compatible);
    if (codes.isEmpty())
        return juce::Result::fail ("key: unknown key '" + term.value + "'");

    out.sql += std::string ("tracks.camelot ") + (term.negated ? "NOT " : "") + "IN (";
    for (int i = 0; i < codes.size(); ++i)
    {
        out.sql += i == 0 ? "?" : ",?";
        out.args.emplace_back (codes[i]);
    }
    out.sql += ")";
    return juce::Result::ok();
}

juce::Result compilePlaylistTerm (const FilterTerm& term, SqlFragment& out)
{
    SqlFragment f;
    if (term.value.startsWithChar ('#'))
    {
        const auto id = term.value.substring (1);
        if (id.isEmpty() || ! id.containsOnly ("0123456789") || id.length() > 15)
            return juce::Result::fail ("playlist: bad id '" + term.value + "'");
        f.sql = "tracks.id IN (SELECT track_id FROM playlist_entries WHERE playlist_id = ?)";
        f.args.emplace_back (static_cast<std::int64_t> (id.getLargeIntValue()));
    }
    else
    {
        f.sql = "tracks.id IN (SELECT e.track_id FROM playlist_entries e JOIN playlists p ON p.id = e.playlist_id WHERE ";
        appendTextPredicate ("p.name", term.value, f);
        f.sql += ")";
    }
    out.sql += term.negated ? "NOT (" + f.sql + ")" : f.sql;
    out.args.insert (out.args.end(), f.args.begin(), f.args.end());
    return juce::Result::ok();
}

} // namespace openmixer::library
