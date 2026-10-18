#include "PrepQueueDao.h"

#include <algorithm>

namespace openmixer::library
{
namespace
{
constexpr const char* kColumns = "track_id, position, priority, status, error";

PrepStatus parseStatus (const juce::String& text) noexcept
{
    for (auto s : { PrepStatus::Running, PrepStatus::Done, PrepStatus::Failed, PrepStatus::Cancelled })
        if (text == toString (s))
            return s;
    return PrepStatus::Queued;
}

PrepEntry readEntry (const Statement& row)
{
    PrepEntry e;
    e.trackId = row.int64 (0);
    e.position = row.integer (1);
    e.priority = row.integer (2);
    e.status = parseStatus (row.text (3));
    e.error = row.text (4);
    e.progress = e.status == PrepStatus::Done ? 1.0f : 0.0f;
    return e;
}
} // namespace

const char* toString (PrepStatus status) noexcept
{
    switch (status)
    {
        case PrepStatus::Running:   return "running";
        case PrepStatus::Done:      return "done";
        case PrepStatus::Failed:    return "failed";
        case PrepStatus::Cancelled: return "cancelled";
        case PrepStatus::Queued:    break;
    }
    return "queued";
}

juce::Result PrepQueueDao::enqueue (std::int64_t trackId)
{
    auto s = db.prepare ("INSERT INTO prep_queue (track_id, position, priority, status)"
                         " VALUES (?1, (SELECT COALESCE(MAX(position), -1) + 1 FROM prep_queue), 0, 'queued')"
                         " ON CONFLICT(track_id) DO UPDATE SET position=excluded.position, priority=0,"
                         " status='queued', error='' WHERE prep_queue.status != 'running';");
    s.bindAll (trackId);
    return s.run();
}

juce::Result PrepQueueDao::prioritise (std::int64_t trackId)
{
    auto s = db.prepare ("UPDATE prep_queue SET priority=(SELECT COALESCE(MAX(priority), 0) + 1 FROM prep_queue)"
                         " WHERE track_id=?1 AND status='queued';");
    s.bindAll (trackId);
    return s.run();
}

std::vector<std::int64_t> PrepQueueDao::orderedIds (juce::Result* status)
{
    auto s = db.prepare ("SELECT track_id FROM prep_queue ORDER BY position, id;");
    std::vector<std::int64_t> ids;
    while (s.nextRow())
        ids.push_back (s.int64 (0));
    db.report (s, status);
    return ids;
}

juce::Result PrepQueueDao::move (std::int64_t trackId, int index)
{
    juce::Result read = juce::Result::ok();
    auto ids = orderedIds (&read);
    const auto from = std::find (ids.begin(), ids.end(), trackId);
    if (read.failed() || from == ids.end())
        return read.failed() ? read : juce::Result::fail ("track is not in the prep queue");

    ids.erase (from);
    ids.insert (ids.begin() + juce::jlimit (0, static_cast<int> (ids.size()), index), trackId);

    Transaction tx (db);   // all positions or none
    for (size_t i = 0; i < ids.size(); ++i)
    {
        auto s = db.prepare ("UPDATE prep_queue SET position=?2 WHERE track_id=?1;");
        s.bindAll (ids[i], static_cast<int> (i));
        if (auto r = s.run(); r.failed())
            return r;
    }
    return tx.commit();
}

juce::Result PrepQueueDao::remove (std::int64_t trackId)
{
    auto s = db.prepare ("DELETE FROM prep_queue WHERE track_id=?1;");
    s.bindAll (trackId);
    return s.run();
}

juce::Result PrepQueueDao::clearFinished()
{
    return db.exec ("DELETE FROM prep_queue WHERE status IN ('done', 'failed', 'cancelled');");
}

juce::Result PrepQueueDao::setStatus (std::int64_t trackId, PrepStatus status, const juce::String& error)
{
    auto s = db.prepare ("UPDATE prep_queue SET status=?2, error=?3 WHERE track_id=?1;");
    s.bindAll (trackId, juce::String (toString (status)), error);
    return s.run();
}

juce::Result PrepQueueDao::resetRunning()
{
    return db.exec ("UPDATE prep_queue SET status='queued' WHERE status='running';");
}

std::optional<PrepEntry> PrepQueueDao::find (std::int64_t trackId, juce::Result* status)
{
    auto s = db.prepare (("SELECT " + juce::String (kColumns) + " FROM prep_queue WHERE track_id=?1;").toRawUTF8());
    s.bindAll (trackId);
    std::optional<PrepEntry> entry;
    if (s.nextRow())
        entry = readEntry (s);
    db.report (s, status);
    return entry;
}

std::vector<PrepEntry> PrepQueueDao::all (juce::Result* status)
{
    auto s = db.prepare (("SELECT " + juce::String (kColumns) + " FROM prep_queue ORDER BY position, id;").toRawUTF8());
    std::vector<PrepEntry> entries;
    while (s.nextRow())
        entries.push_back (readEntry (s));
    db.report (s, status);
    return entries;
}

std::optional<PrepEntry> PrepQueueDao::nextQueued (juce::Result* status)
{
    auto s = db.prepare (("SELECT " + juce::String (kColumns) + " FROM prep_queue WHERE status='queued'"
                          " ORDER BY priority DESC, position, id LIMIT 1;").toRawUTF8());
    std::optional<PrepEntry> entry;
    if (s.nextRow())
        entry = readEntry (s);
    db.report (s, status);
    return entry;
}

} // namespace openmixer::library
