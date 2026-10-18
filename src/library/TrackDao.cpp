#include "TrackDao.h"

namespace openmixer::library
{
namespace
{
constexpr const char* kColumns =
    "id, path, content_hash, title, artist, album, genre, year, duration_sec, bpm, musical_key, camelot,"
    " rating, play_count, date_added_ms, last_played_ms, comment";

constexpr const char* kInsertColumns =
    "path, content_hash, title, artist, album, genre, year, duration_sec, bpm, musical_key, camelot,"
    " rating, play_count, date_added_ms, last_played_ms, comment";

constexpr const char* kInsertValues = "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16";

TrackRecord readRow (const Statement& s)
{
    TrackRecord t;
    t.id = s.int64 (0);
    t.path = s.text (1);
    t.contentHash = s.text (2);
    t.title = s.text (3);
    t.artist = s.text (4);
    t.album = s.text (5);
    t.genre = s.text (6);
    t.year = s.integer (7);
    t.durationSec = s.real (8);
    t.bpm = s.optReal (9);
    t.key = s.text (10);
    t.camelot = s.text (11);
    t.rating = s.integer (12);
    t.playCount = s.integer (13);
    t.dateAddedMs = s.int64 (14);
    t.lastPlayedMs = s.optInt64 (15);
    t.comment = s.text (16);
    return t;
}

void bindFields (Statement& s, const TrackRecord& t, std::int64_t dateAdded)
{
    s.bindAll (t.path, t.contentHash, t.title, t.artist, t.album, t.genre, t.year, t.durationSec, t.bpm,
               t.key, t.camelot, t.rating, t.playCount, dateAdded, t.lastPlayedMs, t.comment);
}

std::int64_t addedOrNow (const TrackRecord& t)
{
    return t.dateAddedMs != 0 ? t.dateAddedMs : static_cast<std::int64_t> (juce::Time::currentTimeMillis());
}
} // namespace

juce::Result TrackDao::runChecked (Statement& s)
{
    if (auto r = s.run(); r.failed())
        return r;
    return db.changes() > 0 ? juce::Result::ok() : juce::Result::fail ("no such track");
}

std::optional<std::int64_t> TrackDao::insert (const TrackRecord& track)
{
    auto s = db.prepare (("INSERT INTO tracks (" + juce::String (kInsertColumns) + ") VALUES ("
                          + kInsertValues + ");").toRawUTF8());
    bindFields (s, track, addedOrNow (track));
    if (s.run().failed())
        return std::nullopt;
    return db.lastInsertRowId();
}

std::optional<std::int64_t> TrackDao::upsertByPath (const TrackRecord& track)
{
    Transaction tx (db);
    if (auto existing = findByPath (track.path))
    {
        auto merged = track;
        merged.id = existing->id;
        if (update (merged).failed() || tx.commit().failed())
            return std::nullopt;
        return existing->id;
    }
    auto id = insert (track);
    if (! id.has_value() || tx.commit().failed())
        return std::nullopt;
    return id;
}

juce::Result TrackDao::update (const TrackRecord& t)
{
    auto s = db.prepare ("UPDATE tracks SET content_hash=?2, title=?3, artist=?4, album=?5, genre=?6, year=?7,"
                         " duration_sec=?8, bpm=?9, musical_key=?10, camelot=?11, comment=?12 WHERE id=?1;");
    s.bindAll (t.id, t.contentHash, t.title, t.artist, t.album, t.genre, t.year, t.durationSec, t.bpm,
               t.key, t.camelot, t.comment);
    return runChecked (s);
}

juce::Result TrackDao::remove (std::int64_t id)
{
    auto s = db.prepare ("DELETE FROM tracks WHERE id=?1;");
    s.bindAll (id);
    return runChecked (s);
}

std::vector<TrackRecord> TrackDao::query (const char* where, const juce::String& arg)
{
    auto s = db.prepare (("SELECT " + juce::String (kColumns) + " FROM tracks " + where + ";").toRawUTF8());
    if (arg.isNotEmpty())
        s.bindAll (arg);

    std::vector<TrackRecord> rows;
    while (s.nextRow())
        rows.push_back (readRow (s));
    return rows;
}

std::optional<TrackRecord> TrackDao::findById (std::int64_t id)
{
    auto rows = query (("WHERE id = " + juce::String (id)).toRawUTF8(), {});
    return rows.empty() ? std::nullopt : std::optional<TrackRecord> (rows.front());
}

std::optional<TrackRecord> TrackDao::findByPath (const juce::String& path)
{
    auto rows = query ("WHERE path = ?1", path);
    return rows.empty() ? std::nullopt : std::optional<TrackRecord> (rows.front());
}

std::vector<TrackRecord> TrackDao::findByContentHash (const juce::String& hash)
{
    return query ("WHERE content_hash = ?1 ORDER BY id", hash);
}

std::vector<TrackRecord> TrackDao::all() { return query ("ORDER BY id", {}); }

int TrackDao::count()
{
    auto s = db.prepare ("SELECT COUNT(*) FROM tracks;");
    return s.nextRow() ? s.integer (0) : 0;
}

juce::Result TrackDao::setAnalysis (std::int64_t id, double bpm, const juce::String& key, const juce::String& camelot)
{
    auto s = db.prepare ("UPDATE tracks SET bpm=?2, musical_key=?3, camelot=?4 WHERE id=?1;");
    s.bindAll (id, bpm, key, camelot);
    return runChecked (s);
}

juce::Result TrackDao::setRating (std::int64_t id, int rating)
{
    auto s = db.prepare ("UPDATE tracks SET rating=?2 WHERE id=?1;");
    s.bindAll (id, rating);
    return runChecked (s);
}

juce::Result TrackDao::recordPlay (std::int64_t id, std::int64_t whenMs)
{
    auto s = db.prepare ("UPDATE tracks SET play_count = play_count + 1, last_played_ms=?2 WHERE id=?1;");
    s.bindAll (id, whenMs);
    return runChecked (s);
}

} // namespace openmixer::library
