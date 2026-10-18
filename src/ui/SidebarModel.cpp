#include "SidebarModel.h"

namespace openmixer::ui
{

namespace
{
    SidebarNode node (SidebarNode::Kind kind, const juce::String& label)
    {
        SidebarNode n;
        n.kind = kind;
        n.label = label;
        return n;
    }

    // Leaf name, or "leaf (parent)" when another watch folder shares the leaf, so rows stay distinguishable.
    juce::String watchLabel (const juce::String& path, const std::vector<juce::String>& all)
    {
        const juce::File f (path);
        int same = 0;
        for (const auto& other : all)
            same += juce::File (other).getFileName() == f.getFileName() ? 1 : 0;
        return same > 1 ? f.getFileName() + " (" + f.getParentDirectory().getFullPathName() + ")" : f.getFileName();
    }

    bool addPlaylists (library::PlaylistDao& dao, std::optional<std::int64_t> parent, std::vector<SidebarNode>& out, int depth)
    {
        constexpr int kMaxDepth = 16;   // folders cannot cycle, but a corrupt file must not recurse forever
        juce::Result status = juce::Result::ok();
        const auto items = dao.children (parent, &status);
        if (status.failed() || depth > kMaxDepth)
            return false;
        for (const auto& p : items)
        {
            auto n = node (p.isFolder ? SidebarNode::Kind::Folder : SidebarNode::Kind::Playlist, p.name);
            n.playlistId = p.id;
            if (p.isFolder && ! addPlaylists (dao, p.id, n.children, depth + 1))
                return false;
            out.push_back (std::move (n));
        }
        return true;
    }
}

juce::String SidebarNode::scopeQuery() const
{
    switch (kind)
    {
        case Kind::Playlist: return "playlist:#" + juce::String (playlistId);
        case Kind::WatchFolder: return "folder:\"" + path.removeCharacters ("\"") + "\"";   // anchored prefix, not a substring
        default: return {};
    }
}

std::vector<SidebarNode> buildSidebar (library::Library& library)
{
    std::vector<SidebarNode> top;
    top.push_back (node (SidebarNode::Kind::AllTracks, "All tracks"));

    auto playlists = node (SidebarNode::Kind::PlaylistGroup, "Playlists");
    if (! addPlaylists (library.playlists(), std::nullopt, playlists.children, 0))
        playlists.children.clear();
    top.push_back (std::move (playlists));

    auto watch = node (SidebarNode::Kind::WatchGroup, "Watch folders");
    const auto paths = library.watchFolders().list();
    for (const auto& path : paths)
    {
        auto n = node (SidebarNode::Kind::WatchFolder, watchLabel (path, paths));
        n.path = path;
        watch.children.push_back (std::move (n));
    }
    top.push_back (std::move (watch));

    top.push_back (node (SidebarNode::Kind::PrepQueue, "Prep queue"));
    return top;
}

} // namespace openmixer::ui
