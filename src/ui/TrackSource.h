// Where the browser table gets its rows: a counted, pageable list of tracks. SearchTrackSource runs the
// query syntax against the library (scoped to a playlist or everything); PrepQueueSource lists the
// prep queue in order. Message thread only, same thread as the Library connection they read.
#pragma once

#include "library/PrepQueue.h"
#include "library/SearchEngine.h"

#include <vector>

namespace openmixer::ui
{

class TrackSource
{
public:
    virtual ~TrackSource() = default;
    virtual int count() = 0;
    virtual std::vector<library::TrackRecord> fetch (int offset, int limit) = 0;
    // Why the last count/fetch failed or the query text was rejected; empty when fine.
    virtual juce::String error() const { return {}; }
};

class SearchTrackSource final : public TrackSource
{
public:
    // scope: a query fragment that always applies ("" = all tracks, "playlist:#3"); text: the user's search box.
    // sortField empty keeps any sort: token from the text, else library order.
    SearchTrackSource (library::SearchEngine& engine, const juce::String& scope, const juce::String& text,
                       const juce::String& sortField = {}, bool descending = false);

    int count() override;
    std::vector<library::TrackRecord> fetch (int offset, int limit) override;
    juce::String error() const override { return lastError; }
    const library::SearchQuery& getQuery() const noexcept { return query; }

private:
    void noteStatus (const juce::Result& status);

    library::SearchEngine& search;
    library::SearchQuery query;
    juce::String lastError;
};

class PrepQueueSource final : public TrackSource
{
public:
    PrepQueueSource (library::Library& library, library::PrepQueue& queue);

    int count() override { return static_cast<int> (refresh().size()); }
    std::vector<library::TrackRecord> fetch (int offset, int limit) override;
    juce::String error() const override { return lastError; }

private:
    const std::vector<library::PrepEntry>& refresh();

    library::Library& lib;
    library::PrepQueue& prep;
    std::vector<library::PrepEntry> entries;
    juce::String lastError;
};

} // namespace openmixer::ui
