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

void keepIfEmpty (juce::String& read, const juce::String& old)
{
    if (read.isEmpty())
        read = old;
}

// why: file tags are often sparser than imported data (filename-only title, no artist), so never blank or downgrade it
void mergeFillOnly (const TrackRecord& old, const juce::File& file, TrackRecord& rec)
{
    if (old.title.isNotEmpty() && rec.title == file.getFileNameWithoutExtension())
        rec.title = old.title;
    keepIfEmpty (rec.title, old.title);
    keepIfEmpty (rec.artist, old.artist);
    keepIfEmpty (rec.album, old.album);
    keepIfEmpty (rec.genre, old.genre);
    keepIfEmpty (rec.comment, old.comment);
    if (rec.year == 0)
        rec.year = old.year;
    if (rec.durationSec <= 0.0)
        rec.durationSec = old.durationSec;
}

// why: imported rows have no hash yet; their analysis is still the user's, identical bytes keep theirs too
bool analysisStillValid (const TrackRecord& old, const juce::String& hash)
{
    return old.contentHash.isEmpty() || old.contentHash == hash;
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
    if (existing)
        mergeFillOnly (*existing, file, record);
    if (existing && analysisStillValid (*existing, hash))
        carryAnalysis (*existing, record);

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
