// Track table access. Non-owning view of a Database: the Database must outlive the Dao and the
// Dao is used from the thread that owns that Database (see Database.h).
#pragma once

#include "Database.h"
#include "TrackRecord.h"

#include <vector>

namespace openmixer::library
{

class TrackDao
{
public:
    explicit TrackDao (Database& database) noexcept : db (database) {}

    // Fails if the path already exists. Fills dateAddedMs with now when it is 0.
    std::optional<std::int64_t> insert (const TrackRecord& track);
    // Insert or refresh by path. File-derived fields are overwritten; id, rating, play count, last
    // played and date added are kept. Returns the row id.
    std::optional<std::int64_t> upsertByPath (const TrackRecord& track);
    juce::Result update (const TrackRecord& track);
    juce::Result remove (std::int64_t id);

    std::optional<TrackRecord> findById (std::int64_t id);
    std::optional<TrackRecord> findByPath (const juce::String& path);
    std::vector<TrackRecord> findByContentHash (const juce::String& hash);
    std::vector<TrackRecord> all();   // ordered by id
    int count();

    // Narrow writers so analysis and playback do not rewrite whole rows.
    juce::Result setAnalysis (std::int64_t id, double bpm, const juce::String& key, const juce::String& camelot);
    juce::Result setRating (std::int64_t id, int rating);
    juce::Result recordPlay (std::int64_t id, std::int64_t whenMs);

private:
    std::vector<TrackRecord> query (const char* where, const juce::String& arg);
    juce::Result runChecked (Statement& s);

    Database& db;
};

} // namespace openmixer::library
