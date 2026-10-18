// Scans a folder tree for audio files and keeps the tracks table in step with it: new files are
// added, changed files (mtime or size) re-read, unchanged files skipped, vanished files flagged
// missing (rows and user data are kept so a returning file restores its rating and cues).
// Threading: runs on the thread that owns the Library it is given.
#pragma once

#include "Library.h"
#include "MetadataReader.h"

#include <functional>
#include <set>

namespace openmixer::library
{

struct IngestSummary
{
    int scanned = 0, added = 0, updated = 0, unchanged = 0, restored = 0, missing = 0, failed = 0;

    IngestSummary& operator+= (const IngestSummary& o);
};

class Ingestor
{
public:
    using StopCheck = std::function<bool()>;   // polled between files; true aborts the scan

    Ingestor (Library& libraryToUse, const MetadataReaderRegistry& readerRegistry)
        : library (libraryToUse), readers (readerRegistry) {}

    // Fails if the folder does not exist (nothing is marked missing then: an unmounted drive is not
    // a deleted library). `summary` is filled even on a stopped or failed scan.
    juce::Result ingestFolder (const juce::File& folder, IngestSummary& summary, const StopCheck& shouldStop = {});

private:
    enum class Outcome { Added, Updated, Unchanged, Restored, Failed };

    Outcome ingestFile (const juce::File& file);
    Outcome refresh (const juce::File& file, const std::optional<TrackRecord>& existing);
    int markMissing (const juce::File& folder, const std::set<juce::String>& seen, const StopCheck& shouldStop);

    Library& library;
    const MetadataReaderRegistry& readers;
};

} // namespace openmixer::library
