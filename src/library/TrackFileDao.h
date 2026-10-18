// Per-track file state used by ingest: last-seen mtime/size (to skip unchanged files) and a
// "missing" flag set when the file vanishes from a watched folder. Same threading rules as TrackDao.
#pragma once

#include "Database.h"

#include <optional>
#include <vector>

namespace openmixer::library
{

struct TrackFileState
{
    std::int64_t mtimeMs = 0;
    std::int64_t sizeBytes = 0;
    bool missing = false;
};

struct TrackFileEntry
{
    std::int64_t trackId = 0;
    juce::String path;
    bool missing = false;
};

class TrackFileDao
{
public:
    explicit TrackFileDao (Database& database) noexcept : db (database) {}

    // Records the file as seen and present.
    juce::Result set (std::int64_t trackId, std::int64_t mtimeMs, std::int64_t sizeBytes);
    juce::Result setMissing (std::int64_t trackId, bool missing);

    std::optional<TrackFileState> find (std::int64_t trackId, juce::Result* status = nullptr);
    // Tracks whose path starts with `prefix` (callers pass a folder path ending in a separator).
    std::vector<TrackFileEntry> under (const juce::String& prefix, juce::Result* status = nullptr);
    int missingCount (juce::Result* status = nullptr);

private:
    Database& db;
};

} // namespace openmixer::library
