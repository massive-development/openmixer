// Importer-neutral view of a foreign library: what an importer parses and the writer merges.
// Plain values only, so importers stay free of database code and are testable on their own.
#pragma once

#include "TrackRecord.h"

#include <juce_core/juce_core.h>

#include <cstdint>
#include <vector>

namespace openmixer::library
{

struct ImportedCue
{
    enum class Kind { Cue, Loop };

    Kind kind = Kind::Cue;
    int num = -1;                  // hotcue slot 0.., or -1 for a memory cue/loop
    double startSec = 0.0;
    double endSec = 0.0;           // loops only
    juce::String label;
    std::uint32_t colourArgb = 0;
};

struct ImportedTrack
{
    juce::String key;              // id the source's playlists use to refer to this track
    TrackRecord record;            // record.path is the decoded local path
    double sampleRate = 0.0;       // 0 = unknown (the writer assumes 44100)
    std::vector<ImportedCue> cues;
};

struct ImportedPlaylist
{
    juce::String name;
    bool isFolder = false;
    std::vector<ImportedPlaylist> children;   // folders only
    std::vector<juce::String> trackKeys;      // playlists only, in order
};

struct ImportedLibrary
{
    std::vector<ImportedTrack> tracks;
    std::vector<ImportedPlaylist> playlists;  // top level
};

// Outcome counts. Skipped = entries that cannot map to a local file; errors = malformed data or
// a failed database write (which rolls the whole import back and zeroes the other counts).
struct ImportReport
{
    int tracksAdded = 0, tracksMerged = 0, tracksSkipped = 0;
    int cuesImported = 0, playlistsCreated = 0, entriesAdded = 0, entriesSkipped = 0;
    int errors = 0;
    juce::StringArray messages;    // first few problems, human readable

    void note (const juce::String& message);
};

} // namespace openmixer::library
