#include "SearchParser.h"

namespace openmixer::library
{
namespace
{
struct RawToken { juce::String text; bool negated = false; };

void flush (std::vector<RawToken>& out, juce::String& current, bool& negated, bool& started)
{
    if (started && (current.isNotEmpty() || negated))
        out.push_back ({ current, negated });
    current = {};
    negated = false;
    started = false;
}

// Splits on whitespace outside double quotes; a leading '-' (before the quotes) negates the token.
std::vector<RawToken> tokenize (const juce::String& text)
{
    std::vector<RawToken> out;
    juce::String current;
    bool quoted = false, negated = false, started = false;
    for (auto c : text)
    {
        if (c == '"') { quoted = ! quoted; started = true; continue; }
        if (! quoted && juce::CharacterFunctions::isWhitespace (c)) { flush (out, current, negated, started); continue; }
        if (! started && c == '-') { negated = true; started = true; continue; }
        started = true;
        current += juce::String::charToString (c);
    }
    flush (out, current, negated, started);
    return out;
}

bool parseCount (const juce::String& s, int& out)
{
    if (s.isEmpty() || ! s.containsOnly ("0123456789") || s.length() > 9)
        return false;
    out = s.getIntValue();
    return true;
}

void applyReserved (const juce::String& name, const juce::String& value, SearchQuery& q, juce::StringArray& errors)
{
    if (name == "sort")
    {
        q.descending = value.startsWithChar ('-');
        q.sortField = (q.descending ? value.substring (1) : value).toLowerCase();
        return;
    }
    int n = 0;
    if (! parseCount (value, n))
        errors.add (name + ": expected a non-negative number, got '" + value + "'");
    else if (name == "limit")
        q.limit = n;
    else
        q.offset = n;
}

bool isReserved (const juce::String& name) { return name == "sort" || name == "limit" || name == "offset"; }

void addToken (const RawToken& tok, const SearchFieldRegistry& fields, ParsedQuery& out)
{
    const auto colon = tok.text.indexOfChar (':');
    const auto name = colon > 0 ? tok.text.substring (0, colon).toLowerCase() : juce::String();
    const auto value = colon > 0 ? tok.text.substring (colon + 1) : tok.text;

    if (isReserved (name) && ! tok.negated)
        applyReserved (name, value, out.query, out.errors);
    else if (fields.find (name) != nullptr)
        out.query.terms.push_back ({ name, value, tok.negated });
    else
        out.query.terms.push_back ({ {}, tok.text, tok.negated });
}
} // namespace

ParsedQuery parseQuery (const juce::String& text, const SearchFieldRegistry& fields)
{
    ParsedQuery out;
    for (const auto& tok : tokenize (text))
        if (tok.text.isNotEmpty())
            addToken (tok, fields, out);
    return out;
}

} // namespace openmixer::library
