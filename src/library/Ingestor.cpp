#include "Ingestor.h"

#include "analysis/ClipSource.h"

namespace openmixer::library
{
namespace
{
std::int64_t mtimeMs (const juce::File& f) { return f.getLastModificationTime().toMilliseconds(); }

juce::String folderPrefix (const juce::File& folder)
{
    auto path = folder.getFullPathName();
    return path.endsWithChar (juce::File::getSeparatorChar()) ? path : path + juce::File::getSeparatorString();
}

TrackRecord toRecord (const juce::File& file, const TrackMetadata& m, const juce::String& hash)
{
    TrackRecord t;
    t.path = file.getFullPathName();
    t.contentHash = hash;
    t.title = m.title;
    t.artist = m.artist;
    t.album = m.album;
    t.genre = m.genre;
    t.comment = m.comment;
    t.year = m.year;
    t.durationSec = m.durationSec;
    return t;
}

void carryAnalysis (const TrackRecord& from, TrackRecord& to)
{
    to.bpm = from.bpm;
    to.key = from.key;
    to.camelot = from.camelot;
}
} // namespace

IngestSummary& IngestSummary::operator+= (const IngestSummary& o)
{
    scanned += o.scanned; added += o.added; updated += o.updated; unchanged += o.unchanged;
    restored += o.restored; missing += o.missing; failed += o.failed;
    return *this;
}

Ingestor::Outcome Ingestor::refresh (const juce::File& file, const std::optional<TrackRecord>& existing,
                                     const FileStamp& stamp)
{
    TrackMetadata meta;
    const auto hash = analysis::hashFileContent (file);
    auto* reader = readers.find (file);
    if (reader == nullptr || hash.isEmpty() || reader->read (file, meta).failed())
        return Outcome::Failed;

    auto record = toRecord (file, meta, hash);
    if (existing && existing->contentHash == hash)
        carryAnalysis (*existing, record);   // identical bytes: BPM/key are still valid

    Transaction tx (library.database());
    const auto id = library.tracks().upsertByPath (record);
    if (! id || library.trackFiles().set (*id, stamp.mtimeMs, stamp.sizeBytes).failed() || tx.commit().failed())
        return Outcome::Failed;
    return existing ? Outcome::Updated : Outcome::Added;
}

Ingestor::Outcome Ingestor::ingestFile (const juce::File& file)
{
    juce::Result read = juce::Result::ok();
    const auto existing = library.tracks().findByPath (file.getFullPathName(), &read);
    if (read.failed())
        return Outcome::Failed;   // a failed lookup is not "new file": would duplicate or clobber rows

    std::optional<TrackFileState> state;
    if (existing)
        state = library.trackFiles().find (existing->id, &read);
    if (read.failed())
        return Outcome::Failed;

    // why: stamped before hashing, so an edit during the read is caught by the next scan
    const FileStamp stamp { mtimeMs (file), file.getSize() };
    const bool unchanged = state && state->mtimeMs == stamp.mtimeMs && state->sizeBytes == stamp.sizeBytes;
    if (! unchanged)
        return refresh (file, existing, stamp);
    if (! state->missing)
        return Outcome::Unchanged;
    return library.trackFiles().setMissing (existing->id, false).wasOk() ? Outcome::Restored : Outcome::Failed;
}

int Ingestor::markMissing (const juce::File& folder, const std::set<juce::String>& seen, const StopCheck& shouldStop)
{
    juce::Result read = juce::Result::ok();
    const auto entries = library.trackFiles().under (folderPrefix (folder), &read);
    int flagged = 0;
    for (const auto& e : entries)
    {
        if (read.failed() || (shouldStop && shouldStop()))
            break;   // a partial scan must not flag files it never got to
        if (! e.missing && seen.count (e.path) == 0 && library.trackFiles().setMissing (e.trackId, true).wasOk())
            ++flagged;
    }
    return flagged;
}

juce::Result Ingestor::ingestFolder (const juce::File& folder, IngestSummary& summary, const StopCheck& shouldStop)
{
    if (! folder.isDirectory())
        return juce::Result::fail ("not a folder: " + folder.getFullPathName());

    std::set<juce::String> seen;
    for (const auto& entry : juce::RangedDirectoryIterator (folder, true, "*", juce::File::findFiles))
    {
        if (shouldStop && shouldStop())
            return juce::Result::fail ("stopped");
        const auto file = entry.getFile();
        if (! readers.supports (file))
            continue;
        seen.insert (file.getFullPathName());
        ++summary.scanned;
        switch (ingestFile (file))
        {
            case Outcome::Added: ++summary.added; break;
            case Outcome::Updated: ++summary.updated; break;
            case Outcome::Unchanged: ++summary.unchanged; break;
            case Outcome::Restored: ++summary.restored; break;
            case Outcome::Failed: ++summary.failed; break;   // stays "seen": a transient failure must not flag the file missing
        }
    }
    summary.missing += markMissing (folder, seen, shouldStop);
    return juce::Result::ok();
}

} // namespace openmixer::library
