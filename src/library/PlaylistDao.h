// Playlists, folders (nesting) and ordered entries. Siblings and entries keep dense positions 0..n-1.
// Non-owning view of a Database; use from the thread that owns it (see Database.h).
#pragma once

#include "Database.h"
#include "OrderedList.h"

#include <vector>

namespace openmixer::library
{

struct Playlist
{
    std::int64_t id = 0;
    std::optional<std::int64_t> parentId;   // unset = top level
    juce::String name;
    bool isFolder = false;                  // folders hold playlists/folders, playlists hold tracks
    int position = 0;                       // among siblings
};

struct PlaylistEntry
{
    std::int64_t id = 0;                    // entry id: the same track may appear more than once
    std::int64_t playlistId = 0;
    std::int64_t trackId = 0;
    int position = 0;
};

class PlaylistDao
{
public:
    explicit PlaylistDao (Database& database) noexcept;

    // Appends under parent (unset = top level). The parent must be a folder.
    std::optional<std::int64_t> create (const juce::String& name, std::optional<std::int64_t> parentId = std::nullopt,
                                        bool isFolder = false);
    juce::Result rename (std::int64_t id, const juce::String& name);
    // Deletes the node, everything below it, and closes the sibling gap.
    juce::Result remove (std::int64_t id);
    // Re-parents and/or reorders. Fails when the new parent is not a folder or is inside the node.
    juce::Result move (std::int64_t id, std::optional<std::int64_t> newParentId, int index);

    std::optional<Playlist> find (std::int64_t id);
    std::vector<Playlist> children (std::optional<std::int64_t> parentId);   // ordered by position

    // index < 0 appends. Fails for folders and unknown tracks.
    std::optional<std::int64_t> addTrack (std::int64_t playlistId, std::int64_t trackId, int index = -1);
    juce::Result removeEntry (std::int64_t entryId);
    juce::Result moveEntry (std::int64_t entryId, int newIndex);
    std::optional<PlaylistEntry> findEntry (std::int64_t entryId);
    std::vector<PlaylistEntry> entries (std::int64_t playlistId);   // ordered by position

private:
    juce::Result detach (const Playlist& node);
    bool wouldCycle (std::int64_t id, std::optional<std::int64_t> newParentId);
    bool acceptsChildren (std::optional<std::int64_t> parentId);

    Database& db;
    OrderedList nodes;
    OrderedList items;
};

} // namespace openmixer::library
