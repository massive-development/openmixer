#include "TrackSource.h"

#include <algorithm>

namespace openmixer::ui
{

SearchTrackSource::SearchTrackSource (library::SearchEngine& engine, const juce::String& scope, const juce::String& text,
                                      const juce::String& sortField, bool descending)
    : search (engine)
{
    auto parsed = search.parse (scope + " " + text);
    query = std::move (parsed.query);
    if (sortField.isNotEmpty())
    {
        query.sortField = sortField;
        query.descending = descending;
    }
    if (parsed.errors.size() > 0)
        lastError = parsed.errors.joinIntoString ("; ");
}

void SearchTrackSource::noteStatus (const juce::Result& status)
{
    if (status.failed())
        lastError = status.getErrorMessage();
}

int SearchTrackSource::count()
{
    juce::Result status = juce::Result::ok();
    const int n = search.count (query, &status);
    noteStatus (status);
    return std::max (0, n);
}

std::vector<library::TrackRecord> SearchTrackSource::fetch (int offset, int limit)
{
    auto page = query;
    page.offset = offset;
    page.limit = limit;
    juce::Result status = juce::Result::ok();
    auto rows = search.search (page, &status);
    noteStatus (status);
    return rows;
}

PrepQueueSource::PrepQueueSource (library::Library& library, library::PrepQueue& queue) : lib (library), prep (queue) {}

const std::vector<library::PrepEntry>& PrepQueueSource::refresh()
{
    juce::Result status = juce::Result::ok();
    entries = prep.list (&status);
    if (status.failed())
        lastError = status.getErrorMessage();
    return entries;
}

std::vector<library::TrackRecord> PrepQueueSource::fetch (int offset, int limit)
{
    std::vector<library::TrackRecord> rows;
    const int end = std::min (static_cast<int> (entries.size()), offset + limit);
    for (int i = std::max (0, offset); i < end; ++i)
        if (auto t = lib.tracks().findById (entries[static_cast<size_t> (i)].trackId))
            rows.push_back (std::move (*t));
    return rows;
}

} // namespace openmixer::ui
