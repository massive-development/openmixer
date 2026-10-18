// Rekordbox XML export (DJ_PLAYLISTS): COLLECTION/TRACK with TEMPO and POSITION_MARK cues, and the
// PLAYLISTS NODE tree (folders and playlists). POSITION_MARK Type 0 = cue (Num -1 memory cue,
// 0..7 hotcue), Type 4 = loop; other types (fade, load) are ignored. Times are seconds.
#pragma once

#include "LibraryImporter.h"

namespace openmixer::library
{

class RekordboxImporter : public LibraryImporter
{
public:
    juce::String name() const override { return "Rekordbox XML"; }
    bool recognises (const juce::XmlElement& root) const override { return root.hasTagName ("DJ_PLAYLISTS"); }
    juce::Result parse (const juce::XmlElement& root, ImportedLibrary& out, ImportReport& report) const override;
};

} // namespace openmixer::library
