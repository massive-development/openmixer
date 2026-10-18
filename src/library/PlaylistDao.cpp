#include "PlaylistDao.h"

namespace openmixer::library
{
namespace
{
Playlist readPlaylist (const Statement& s)
{
    return { s.int64 (0), s.optInt64 (1), s.text (2), s.integer (3) != 0, s.integer (4) };
}

PlaylistEntry readEntry (const Statement& s)
{
    return { s.int64 (0), s.int64 (1), s.int64 (2), s.integer (3) };
}

juce::Result firstFailure (juce::Result a, const juce::Result& b)
{
    return a.failed() ? a : b;
}
} // namespace

PlaylistDao::PlaylistDao (Database& database) noexcept
    : db (database), nodes (database, "playlists", "parent_id"), items (database, "playlist_entries", "playlist_id")
{
}

bool PlaylistDao::acceptsChildren (std::optional<std::int64_t> parentId)
{
    if (! parentId.has_value())
        return true;
    auto parent = find (*parentId);
    return parent.has_value() && parent->isFolder;
}

std::optional<std::int64_t> PlaylistDao::create (const juce::String& name, std::optional<std::int64_t> parentId, bool isFolder)
{
    if (! acceptsChildren (parentId))
        return std::nullopt;

    auto s = db.prepare ("INSERT INTO playlists (parent_id, name, is_folder, position) VALUES (?1,?2,?3,?4);");
    s.bindAll (parentId, name, isFolder ? 1 : 0, nodes.count (parentId));
    if (s.run().failed())
        return std::nullopt;
    return db.lastInsertRowId();
}

juce::Result PlaylistDao::rename (std::int64_t id, const juce::String& name)
{
    auto s = db.prepare ("UPDATE playlists SET name=?2 WHERE id=?1;");
    s.bindAll (id, name);
    if (auto r = s.run(); r.failed())
        return r;
    return db.changes() > 0 ? juce::Result::ok() : juce::Result::fail ("no such playlist");
}

juce::Result PlaylistDao::detach (const Playlist& node)
{
    auto s = db.prepare ("DELETE FROM playlists WHERE id=?1;");
    s.bindAll (node.id);
    return firstFailure (s.run(), nodes.closeGap (node.parentId, node.position));
}

juce::Result PlaylistDao::remove (std::int64_t id)
{
    auto node = find (id);
    if (! node.has_value())
        return juce::Result::fail ("no such playlist");

    Transaction tx (db);
    if (auto r = detach (*node); r.failed())
        return r;
    return tx.commit();
}

bool PlaylistDao::wouldCycle (std::int64_t id, std::optional<std::int64_t> newParentId)
{
    for (auto cursor = newParentId; cursor.has_value();)
    {
        if (*cursor == id)
            return true;
        auto ancestor = find (*cursor);
        cursor = ancestor.has_value() ? ancestor->parentId : std::nullopt;
    }
    return false;
}

juce::Result PlaylistDao::move (std::int64_t id, std::optional<std::int64_t> newParentId, int index)
{
    auto node = find (id);
    if (! node.has_value())
        return juce::Result::fail ("no such playlist");
    if (! acceptsChildren (newParentId) || wouldCycle (id, newParentId))
        return juce::Result::fail ("invalid destination");

    Transaction tx (db);
    const auto sameParent = node->parentId == newParentId;
    if (! sameParent)
        if (auto r = nodes.closeGap (node->parentId, node->position); r.failed())
            return r;

    const auto target = sameParent ? nodes.moveTarget (newParentId, index) : nodes.insertIndex (newParentId, index);
    auto gap = sameParent ? nodes.shiftForMove (newParentId, node->position, target) : nodes.openGap (newParentId, target);
    auto set = db.prepare ("UPDATE playlists SET parent_id=?2, position=?3 WHERE id=?1;");
    set.bindAll (id, newParentId, target);
    if (auto r = firstFailure (gap, set.run()); r.failed())
        return r;
    return tx.commit();
}

std::optional<Playlist> PlaylistDao::find (std::int64_t id)
{
    auto s = db.prepare ("SELECT id, parent_id, name, is_folder, position FROM playlists WHERE id=?1;");
    s.bindAll (id);
    return s.nextRow() ? std::optional<Playlist> (readPlaylist (s)) : std::nullopt;
}

std::vector<Playlist> PlaylistDao::children (std::optional<std::int64_t> parentId)
{
    auto s = db.prepare ("SELECT id, parent_id, name, is_folder, position FROM playlists"
                         " WHERE parent_id IS ?1 ORDER BY position;");
    s.bindAll (parentId);

    std::vector<Playlist> out;
    while (s.nextRow())
        out.push_back (readPlaylist (s));
    return out;
}

std::optional<std::int64_t> PlaylistDao::addTrack (std::int64_t playlistId, std::int64_t trackId, int index)
{
    auto list = find (playlistId);
    if (! list.has_value() || list->isFolder)
        return std::nullopt;

    Transaction tx (db);
    const auto at = items.insertIndex (playlistId, index);
    auto insert = db.prepare ("INSERT INTO playlist_entries (playlist_id, track_id, position) VALUES (?1,?2,?3);");
    insert.bindAll (playlistId, trackId, at);
    if (items.openGap (playlistId, at).failed() || insert.run().failed())
        return std::nullopt;

    const auto entryId = db.lastInsertRowId();
    if (tx.commit().failed())
        return std::nullopt;
    return entryId;
}

juce::Result PlaylistDao::removeEntry (std::int64_t entryId)
{
    auto entry = findEntry (entryId);
    if (! entry.has_value())
        return juce::Result::fail ("no such entry");

    Transaction tx (db);
    auto del = db.prepare ("DELETE FROM playlist_entries WHERE id=?1;");
    del.bindAll (entryId);
    if (auto r = firstFailure (del.run(), items.closeGap (entry->playlistId, entry->position)); r.failed())
        return r;
    return tx.commit();
}

juce::Result PlaylistDao::moveEntry (std::int64_t entryId, int newIndex)
{
    auto entry = findEntry (entryId);
    if (! entry.has_value())
        return juce::Result::fail ("no such entry");

    Transaction tx (db);
    const auto target = items.moveTarget (entry->playlistId, newIndex);
    auto set = db.prepare ("UPDATE playlist_entries SET position=?2 WHERE id=?1;");
    set.bindAll (entryId, target);
    const auto shifted = items.shiftForMove (entry->playlistId, entry->position, target);   // before set: order matters
    if (auto r = firstFailure (shifted, set.run()); r.failed())
        return r;
    return tx.commit();
}

std::optional<PlaylistEntry> PlaylistDao::findEntry (std::int64_t entryId)
{
    auto s = db.prepare ("SELECT id, playlist_id, track_id, position FROM playlist_entries WHERE id=?1;");
    s.bindAll (entryId);
    return s.nextRow() ? std::optional<PlaylistEntry> (readEntry (s)) : std::nullopt;
}

std::vector<PlaylistEntry> PlaylistDao::entries (std::int64_t playlistId)
{
    auto s = db.prepare ("SELECT id, playlist_id, track_id, position FROM playlist_entries"
                         " WHERE playlist_id=?1 ORDER BY position;");
    s.bindAll (playlistId);

    std::vector<PlaylistEntry> out;
    while (s.nextRow())
        out.push_back (readEntry (s));
    return out;
}

} // namespace openmixer::library
