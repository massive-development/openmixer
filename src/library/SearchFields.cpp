#include "SearchCompiler.h"

namespace openmixer::library
{
namespace
{
SearchField simple (const char* name, FieldKind kind, const char* column, bool nocaseSort = false)
{
    SearchField f;
    f.name = name;
    f.kind = kind;
    f.column = column;
    f.sortColumns = { nocaseSort ? std::string (column) + " COLLATE NOCASE" : std::string (column) };
    return f;
}

SearchField custom (const char* name, std::vector<std::string> sortBy,
                    std::function<juce::Result (const FilterTerm&, SqlFragment&)> fn)
{
    SearchField f;
    f.name = name;
    f.kind = FieldKind::Custom;
    f.sortColumns = std::move (sortBy);
    f.compile = std::move (fn);
    return f;
}
} // namespace

const std::vector<std::string>& freeTextColumns()
{
    static const std::vector<std::string> columns {
        "tracks.title", "tracks.artist", "tracks.album", "tracks.genre", "tracks.comment" };
    return columns;
}

SearchFieldRegistry SearchFieldRegistry::withDefaults()
{
    SearchFieldRegistry r;
    for (const char* name : { "title", "artist", "album", "genre", "comment", "path" })
        r.add (simple (name, FieldKind::Text, (std::string ("tracks.") + name).c_str(), true));
    r.add (simple ("year", FieldKind::Number, "tracks.year"));
    r.add (simple ("bpm", FieldKind::Number, "tracks.bpm"));
    r.add (simple ("rating", FieldKind::Number, "tracks.rating"));
    r.add (simple ("plays", FieldKind::Number, "tracks.play_count"));
    r.add (simple ("duration", FieldKind::Number, "tracks.duration_sec"));
    r.add (simple ("added", FieldKind::Date, "tracks.date_added_ms"));
    r.add (simple ("played", FieldKind::Date, "tracks.last_played_ms"));
    r.add (custom ("key", { "CAST(tracks.camelot AS INTEGER)", "tracks.camelot" },
                   [] (const FilterTerm& t, SqlFragment& out) { return compileKeyTerm (t, out); }));
    r.add (custom ("playlist", {}, [] (const FilterTerm& t, SqlFragment& out) { return compilePlaylistTerm (t, out); }));
    return r;
}

void SearchFieldRegistry::add (SearchField field)
{
    field.name = field.name.toLowerCase();
    for (auto& existing : fields)
        if (existing.name == field.name)
        {
            existing = std::move (field);
            return;
        }
    fields.push_back (std::move (field));
}

const SearchField* SearchFieldRegistry::find (const juce::String& name) const
{
    const auto key = name.toLowerCase();
    for (const auto& f : fields)
        if (f.name == key)
            return &f;
    return nullptr;
}

juce::StringArray SearchFieldRegistry::names() const
{
    juce::StringArray out;
    for (const auto& f : fields)
        out.add (f.name);
    return out;
}

} // namespace openmixer::library
