// Writes an ImportedLibrary into the database in a single Transaction, merging by path (see
// ImportMerge.h). Any database failure rolls everything back, so a half-imported library never exists.
// Playlists merge by name under the same parent, so re-importing adds no duplicate playlists or entries.
// Threading: runs on the thread that owns the Library.
#pragma once

#include "ImportModel.h"
#include "Library.h"

#include <map>
#include <set>

namespace openmixer::library
{

class ImportWriter
{
public:
    ImportWriter (Library& libraryToUse, ImportReport& reportToFill) : library (libraryToUse), report (reportToFill) {}

    // On failure the library is unchanged; counts from this write are dropped and one error is added.
    juce::Result write (const ImportedLibrary& data);

private:
    juce::Result writeAll (const ImportedLibrary& data);
    juce::Result writeTrack (const ImportedTrack& track);
    juce::Result updateExisting (TrackRecord& local, const TrackRecord& imported);
    juce::Result writeCues (std::int64_t trackId, const ImportedTrack& track);
    juce::Result writePlaylists (const std::vector<ImportedPlaylist>& nodes, std::optional<std::int64_t> parent);
    juce::Result writePlaylist (const ImportedPlaylist& node, std::optional<std::int64_t> parent);
    juce::Result fillPlaylist (std::int64_t playlistId, const ImportedPlaylist& node);

    Library& library;
    ImportReport& report;
    std::map<juce::String, std::int64_t> idsByKey;
};

} // namespace openmixer::library
