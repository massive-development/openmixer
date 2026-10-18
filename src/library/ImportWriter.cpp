#include "ImportWriter.h"

#include "ImportCues.h"
#include "ImportMerge.h"

namespace openmixer::library
{
namespace
{
juce::Result failure (Library& lib, const char* what)
{
    return juce::Result::fail (juce::String (what) + ": " + lib.database().lastError());
}
} // namespace

juce::Result ImportWriter::write (const ImportedLibrary& data)
{
    Transaction tx (library.database());
    const ImportReport before = report;
    auto result = writeAll (data);
    if (result.wasOk())
        result = tx.commit();
    if (result.failed())
    {
        report = before;                         // nothing was committed, so nothing is counted
        ++report.errors;
        report.note (result.getErrorMessage());
    }
    return result;
}

juce::Result ImportWriter::writeAll (const ImportedLibrary& data)
{
    for (const auto& track : data.tracks)
        if (auto r = writeTrack (track); r.failed())
            return r;
    return writePlaylists (data.playlists, std::nullopt);
}

juce::Result ImportWriter::writeTrack (const ImportedTrack& track)
{
    auto status = juce::Result::ok();
    auto existing = library.tracks().findByPath (track.record.path, &status);
    if (status.failed())
        return status;                           // a failed lookup must not be read as "new"
    std::optional<std::int64_t> id;
    if (existing.has_value())
    {
        id = existing->id;
        if (auto r = updateExisting (*existing, track.record); r.failed())
            return r;
        ++report.tracksMerged;
    }
    else
    {
        id = library.tracks().insert (track.record);
        if (! id.has_value())
            return failure (library, "Could not insert track");
        ++report.tracksAdded;
    }
    idsByKey[track.key] = *id;
    return writeCues (*id, track);
}

juce::Result ImportWriter::updateExisting (TrackRecord& local, const TrackRecord& imported)
{
    if (! mergeTrackRecord (local, imported))
        return juce::Result::ok();
    auto& dao = library.tracks();
    if (auto r = dao.update (local); r.failed())     // update() leaves user data alone, so write it separately
        return r;
    if (auto r = dao.setRating (local.id, local.rating); r.failed())
        return r;
    return dao.setPlayStats (local.id, local.playCount, local.lastPlayedMs);
}

juce::Result ImportWriter::writeCues (std::int64_t trackId, const ImportedTrack& track)
{
    return importCues (library.cues(), trackId, track, report);
}

juce::Result ImportWriter::writePlaylists (const std::vector<ImportedPlaylist>& nodes, std::optional<std::int64_t> parent)
{
    for (const auto& node : nodes)
        if (auto r = writePlaylist (node, parent); r.failed())
            return r;
    return juce::Result::ok();
}

juce::Result ImportWriter::writePlaylist (const ImportedPlaylist& node, std::optional<std::int64_t> parent)
{
    auto status = juce::Result::ok();
    std::optional<std::int64_t> id;
    for (const auto& sibling : library.playlists().children (parent, &status))
        if (sibling.name == node.name && sibling.isFolder == node.isFolder)
            id = sibling.id;
    if (status.failed())
        return status;
    if (! id.has_value())
    {
        id = library.playlists().create (node.name, parent, node.isFolder);
        if (! id.has_value())
            return failure (library, "Could not create playlist");
        ++report.playlistsCreated;
    }
    return node.isFolder ? writePlaylists (node.children, id) : fillPlaylist (*id, node);
}

juce::Result ImportWriter::fillPlaylist (std::int64_t playlistId, const ImportedPlaylist& node)
{
    auto status = juce::Result::ok();
    std::set<std::int64_t> present;
    for (const auto& e : library.playlists().entries (playlistId, &status))
        present.insert (e.trackId);
    if (status.failed())
        return status;
    for (const auto& key : node.trackKeys)
    {
        const auto it = idsByKey.find (key);
        if (it == idsByKey.end()) { ++report.entriesSkipped; continue; }
        if (! present.insert (it->second).second)
            continue;                            // already there from an earlier import
        if (! library.playlists().addTrack (playlistId, it->second).has_value())
            return failure (library, "Could not add playlist entry");
        ++report.entriesAdded;
    }
    return juce::Result::ok();
}

} // namespace openmixer::library
