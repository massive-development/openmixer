// iTunes / Apple Music "Library.xml" (property list): tracks, ratings, play counts, playlists and
// playlist folders. Cues do not exist in this format. Remote and file-less tracks are skipped.
#pragma once

#include "LibraryImporter.h"

namespace openmixer::library
{

class ItunesImporter : public LibraryImporter
{
public:
    juce::String name() const override { return "iTunes / Music Library.xml"; }
    bool recognises (const juce::XmlElement& root) const override;
    juce::Result parse (const juce::XmlElement& root, ImportedLibrary& out, ImportReport& report) const override;
};

} // namespace openmixer::library
