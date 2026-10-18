#include "ItunesImporter.h"

#include "FileUrl.h"
#include "PlistReader.h"

#include <map>

namespace openmixer::library
{
namespace
{
constexpr int kMaxStars = 5;

const juce::XmlElement* rootDict (const juce::XmlElement& root)
{
    return root.hasTagName ("plist") ? root.getChildByName ("dict") : nullptr;
}

// iTunes stores 0..100 in steps of 20; computed ratings are derived from the album, not user intent.
int starsFrom (const PlistDict& d)
{
    if (d.boolean ("Rating Computed"))
        return 0;
    return static_cast<int> (juce::jlimit<juce::int64> (0, kMaxStars, (d.integer ("Rating") + 10) / 20));
}

bool fillRecord (const PlistDict& d, TrackRecord& r)
{
    if (d.string ("Track Type") == "URL")
        return false;
    r.path = fileUrlToPath (d.string ("Location"));
    r.title = d.string ("Name");
    r.artist = d.string ("Artist");
    r.album = d.string ("Album");
    r.genre = d.string ("Genre");
    r.comment = d.string ("Comments");
    r.year = static_cast<int> (d.integer ("Year"));
    r.durationSec = static_cast<double> (d.integer ("Total Time")) / 1000.0;
    r.rating = starsFrom (d);
    r.playCount = static_cast<int> (juce::jmax<juce::int64> (0, d.integer ("Play Count")));
    r.dateAddedMs = d.dateMs ("Date Added");
    if (const auto played = d.dateMs ("Play Date UTC"); played > 0)
        r.lastPlayedMs = played;
    if (const auto bpm = d.integer ("BPM"); bpm > 0)
        r.bpm = static_cast<double> (bpm);
    return r.path.isNotEmpty();
}

void readTracks (const PlistDict& root, ImportedLibrary& out, ImportReport& report)
{
    const auto* tracks = root.find ("Tracks");
    if (tracks == nullptr)
        return;
    const PlistDict tracksDict (*tracks);
    for (const auto& [id, value] : tracksDict.entries())
    {
        ImportedTrack t;
        t.key = id;
        t.sampleRate = static_cast<double> (PlistDict (*value).integer ("Sample Rate"));
        if (fillRecord (PlistDict (*value), t.record))
            out.tracks.push_back (std::move (t));
        else
        {
            ++report.tracksSkipped;
            report.note ("skipped track " + id + ": no local file location");
        }
    }
}

struct FlatPlaylist
{
    juce::String persistentId, parentId;
    ImportedPlaylist node;
};

bool isBuiltIn (const PlistDict& d)
{
    return d.boolean ("Master") || (d.has ("Distinguished Kind") && ! d.boolean ("Folder"));
}

FlatPlaylist readPlaylist (const PlistDict& d)
{
    FlatPlaylist f;
    f.persistentId = d.string ("Playlist Persistent ID");
    f.parentId = d.string ("Parent Persistent ID");
    f.node.name = d.string ("Name");
    f.node.isFolder = d.boolean ("Folder");
    if (const auto* items = d.find ("Playlist Items"); items != nullptr && ! f.node.isFolder)
        for (auto* item : items->getChildIterator())
            f.node.trackKeys.push_back (juce::String (PlistDict (*item).integer ("Track ID")));
    return f;
}

std::vector<FlatPlaylist> readPlaylists (const PlistDict& root)
{
    std::vector<FlatPlaylist> flat;
    if (const auto* list = root.find ("Playlists"))
        for (auto* p : list->getChildIterator())
            if (! isBuiltIn (PlistDict (*p)))
                flat.push_back (readPlaylist (PlistDict (*p)));
    return flat;
}

// Children of parentId in file order; a playlist whose parent is unknown becomes top level.
std::vector<ImportedPlaylist> assemble (const std::vector<FlatPlaylist>& flat, const juce::String& parentId,
                                        const std::map<juce::String, bool>& known)
{
    std::vector<ImportedPlaylist> out;
    for (const auto& f : flat)
    {
        const bool orphan = parentId.isEmpty() && f.parentId.isNotEmpty() && known.count (f.parentId) == 0;
        if (f.parentId != parentId && ! orphan)
            continue;
        out.push_back (f.node);
        if (f.node.isFolder && f.persistentId.isNotEmpty())
            out.back().children = assemble (flat, f.persistentId, known);
    }
    return out;
}
} // namespace

bool ItunesImporter::recognises (const juce::XmlElement& root) const
{
    const auto* dict = rootDict (root);
    return dict != nullptr && PlistDict (*dict).has ("Tracks");
}

juce::Result ItunesImporter::parse (const juce::XmlElement& root, ImportedLibrary& out, ImportReport& report) const
{
    const auto* dict = rootDict (root);
    if (dict == nullptr)
        return juce::Result::fail ("Not an iTunes property list");
    const PlistDict top (*dict);
    readTracks (top, out, report);
    const auto flat = readPlaylists (top);
    std::map<juce::String, bool> known;
    for (const auto& f : flat)
        known[f.persistentId] = true;
    out.playlists = assemble (flat, {}, known);
    return juce::Result::ok();
}

} // namespace openmixer::library
