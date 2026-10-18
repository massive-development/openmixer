#include "Migrations.h"

namespace openmixer::library
{
namespace
{
constexpr const char* kVersionTable =
    "CREATE TABLE IF NOT EXISTS schema_version ("
    " version INTEGER PRIMARY KEY,"
    " name TEXT NOT NULL,"
    " applied_at INTEGER NOT NULL);";

constexpr const char* kSchemaV1 = R"sql(
CREATE TABLE tracks (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  content_hash TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL DEFAULT '',
  artist TEXT NOT NULL DEFAULT '',
  album TEXT NOT NULL DEFAULT '',
  genre TEXT NOT NULL DEFAULT '',
  year INTEGER NOT NULL DEFAULT 0,
  duration_sec REAL NOT NULL DEFAULT 0,
  bpm REAL,
  musical_key TEXT NOT NULL DEFAULT '',
  camelot TEXT NOT NULL DEFAULT '',
  rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
  play_count INTEGER NOT NULL DEFAULT 0,
  date_added_ms INTEGER NOT NULL DEFAULT 0,
  last_played_ms INTEGER,
  comment TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_tracks_hash ON tracks (content_hash);
CREATE INDEX idx_tracks_artist ON tracks (artist);

CREATE TABLE cues (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  slot INTEGER NOT NULL CHECK (slot >= 0),
  position_frames INTEGER NOT NULL CHECK (position_frames >= 0),
  colour_argb INTEGER NOT NULL DEFAULT 0,
  label TEXT NOT NULL DEFAULT '',
  UNIQUE (track_id, slot)
);

CREATE TABLE loops (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  slot INTEGER NOT NULL CHECK (slot >= 0),
  start_frames INTEGER NOT NULL CHECK (start_frames >= 0),
  end_frames INTEGER NOT NULL,
  colour_argb INTEGER NOT NULL DEFAULT 0,
  label TEXT NOT NULL DEFAULT '',
  CHECK (end_frames > start_frames),
  UNIQUE (track_id, slot)
);

CREATE TABLE playlists (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES playlists (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  is_folder INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_playlists_parent ON playlists (parent_id, position);

CREATE TABLE playlist_entries (
  id INTEGER PRIMARY KEY,
  playlist_id INTEGER NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  position INTEGER NOT NULL
);
CREATE INDEX idx_entries_playlist ON playlist_entries (playlist_id, position);
CREATE INDEX idx_entries_track ON playlist_entries (track_id);
)sql";

constexpr const char* kSchemaV2 = R"sql(
CREATE TABLE track_files (
  track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
  mtime_ms INTEGER NOT NULL,
  size_bytes INTEGER NOT NULL,
  missing INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_track_files_missing ON track_files (missing);

CREATE TABLE watch_folders (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE
);
)sql";

constexpr const char* kSchemaV3 = R"sql(
CREATE INDEX idx_tracks_bpm ON tracks (bpm);
CREATE INDEX idx_tracks_rating ON tracks (rating);
CREATE INDEX idx_tracks_added ON tracks (date_added_ms);
CREATE INDEX idx_tracks_genre ON tracks (genre);
CREATE INDEX idx_tracks_camelot ON tracks (camelot);
)sql";

constexpr const char* kSchemaV4 = R"sql(
CREATE TABLE prep_queue (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL UNIQUE REFERENCES tracks (id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),
  error TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_prep_queue_order ON prep_queue (status, priority DESC, position);
)sql";

bool isStrictlyIncreasing (const std::vector<Migration>& list)
{
    int previous = 0;
    for (const auto& m : list)
    {
        if (m.version <= previous)
            return false;
        previous = m.version;
    }
    return true;
}

juce::Result apply (Database& db, const Migration& m)
{
    Transaction tx (db);
    if (auto r = db.exec (m.sql); r.failed())
        return juce::Result::fail ("migration " + juce::String (m.version) + " (" + m.name + "): " + r.getErrorMessage());

    auto record = db.prepare ("INSERT INTO schema_version (version, name, applied_at) VALUES (?1, ?2, ?3);");
    record.bindAll (m.version, juce::String (m.name), static_cast<std::int64_t> (juce::Time::currentTimeMillis()));
    if (auto r = record.run(); r.failed())
        return r;
    return tx.commit();
}
} // namespace

const std::vector<Migration>& coreMigrations()
{
    static const std::vector<Migration> list {
        { 1, "tracks, cues, loops, playlists", kSchemaV1 },
        { 2, "track file state, watch folders", kSchemaV2 },
        { 3, "search indexes", kSchemaV3 },
        { 4, "track prep queue", kSchemaV4 },
    };
    return list;
}

int currentSchemaVersion (Database& db)
{
    if (! db.exec (kVersionTable).wasOk())
        return -1;

    auto s = db.prepare ("SELECT COALESCE(MAX(version), 0) FROM schema_version;");
    return s.nextRow() ? s.integer (0) : -1;
}

juce::Result migrate (Database& db, const std::vector<Migration>& migrations)
{
    if (! isStrictlyIncreasing (migrations))
        return juce::Result::fail ("migration versions must be strictly increasing");

    const auto current = currentSchemaVersion (db);
    if (current < 0)
        return juce::Result::fail ("cannot read schema version: " + db.lastError());
    if (! migrations.empty() && current > migrations.back().version)
        return juce::Result::fail ("library database is newer than this app (schema " + juce::String (current) + ")");

    for (const auto& m : migrations)
        if (m.version > current)
            if (auto r = apply (db, m); r.failed())
                return r;
    return juce::Result::ok();
}

} // namespace openmixer::library
