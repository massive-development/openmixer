#include "TrackFileDao.h"

namespace openmixer::library
{

juce::Result TrackFileDao::set (std::int64_t trackId, std::int64_t mtimeMs, std::int64_t sizeBytes)
{
    auto s = db.prepare ("INSERT INTO track_files (track_id, mtime_ms, size_bytes, missing) VALUES (?1, ?2, ?3, 0)"
                         " ON CONFLICT(track_id) DO UPDATE SET mtime_ms=?2, size_bytes=?3, missing=0;");
    s.bindAll (trackId, mtimeMs, sizeBytes);
    return s.run();
}

juce::Result TrackFileDao::setMissing (std::int64_t trackId, bool missing)
{
    auto s = db.prepare ("UPDATE track_files SET missing=?2 WHERE track_id=?1;");
    s.bindAll (trackId, missing ? 1 : 0);
    return s.run();
}

std::optional<TrackFileState> TrackFileDao::find (std::int64_t trackId, juce::Result* status)
{
    auto s = db.prepare ("SELECT mtime_ms, size_bytes, missing FROM track_files WHERE track_id=?1;");
    s.bindAll (trackId);
    std::optional<TrackFileState> state;
    if (s.nextRow())
        state = TrackFileState { s.int64 (0), s.int64 (1), s.integer (2) != 0 };
    db.report (s, status);
    return state;
}

std::vector<TrackFileEntry> TrackFileDao::under (const juce::String& prefix, juce::Result* status)
{
    // substr compare instead of LIKE so '%' and '_' in folder names are literal
    auto s = db.prepare ("SELECT t.id, t.path, COALESCE(f.missing, 0) FROM tracks t"
                         " LEFT JOIN track_files f ON f.track_id = t.id"
                         " WHERE substr(t.path, 1, length(?1)) = ?1 ORDER BY t.id;");
    s.bindAll (prefix);
    std::vector<TrackFileEntry> rows;
    while (s.nextRow())
        rows.push_back ({ s.int64 (0), s.text (1), s.integer (2) != 0 });
    db.report (s, status);
    return rows;
}

int TrackFileDao::missingCount (juce::Result* status)
{
    auto s = db.prepare ("SELECT COUNT(*) FROM track_files WHERE missing=1;");
    const auto n = s.nextRow() ? s.integer (0) : 0;
    db.report (s, status);
    return n;
}

} // namespace openmixer::library
