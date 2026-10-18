// Plain-data model of the browser sidebar: All tracks, Playlists (folders nest), Watch folders, Prep
// queue. Built from the library, so the tree contents are testable without a Component.
#pragma once

#include "library/Library.h"

#include <vector>

namespace openmixer::ui
{

struct SidebarNode
{
    enum class Kind { AllTracks, PlaylistGroup, Folder, Playlist, WatchGroup, WatchFolder, PrepQueue };

    Kind kind = Kind::AllTracks;
    juce::String label;
    std::int64_t playlistId = 0;   // Folder / Playlist
    juce::String path;             // WatchFolder
    std::vector<SidebarNode> children;

    bool isSelectable() const noexcept { return kind != Kind::PlaylistGroup && kind != Kind::Folder && kind != Kind::WatchGroup; }
    // Query fragment that scopes the track list to this node ("" = everything); the prep queue has no query.
    juce::String scopeQuery() const;
};

// Top-level nodes in display order. Playlist reads that fail yield an empty group, never a partial tree.
std::vector<SidebarNode> buildSidebar (library::Library& library);

} // namespace openmixer::ui
