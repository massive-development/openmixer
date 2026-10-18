#include "RekordboxImporter.h"

#include "FileUrl.h"

#include <map>

namespace openmixer::library
{
namespace
{
constexpr int kMarkCue = 0, kMarkLoop = 4;
constexpr int kRatingStep = 51;   // Rekordbox stores stars as 0, 51, 102, 153, 204, 255

std::uint32_t colourOf (const juce::XmlElement& mark)
{
    if (! mark.hasAttribute ("Red"))
        return 0;
    return 0xff000000u | (static_cast<std::uint32_t> (mark.getIntAttribute ("Red") & 0xff) << 16)
                       | (static_cast<std::uint32_t> (mark.getIntAttribute ("Green") & 0xff) << 8)
                       | static_cast<std::uint32_t> (mark.getIntAttribute ("Blue") & 0xff);
}

bool readMark (const juce::XmlElement& mark, ImportedCue& cue)
{
    const int type = mark.getIntAttribute ("Type", -1);
    if (type != kMarkCue && type != kMarkLoop)
        return false;                                  // fades and load points have no library home
    cue.kind = type == kMarkLoop ? ImportedCue::Kind::Loop : ImportedCue::Kind::Cue;
    cue.num = mark.getIntAttribute ("Num", -1);
    cue.startSec = mark.getDoubleAttribute ("Start");
    cue.endSec = mark.getDoubleAttribute ("End");
    cue.label = mark.getStringAttribute ("Name");
    cue.colourArgb = colourOf (mark);
    return true;
}

void readMarks (const juce::XmlElement& track, ImportedTrack& out)
{
    for (auto* mark : track.getChildWithTagNameIterator ("POSITION_MARK"))
        if (ImportedCue cue; readMark (*mark, cue))
            out.cues.push_back (cue);
}

double bpmOf (const juce::XmlElement& track)
{
    if (const double avg = track.getDoubleAttribute ("AverageBpm"); avg > 0.0)
        return avg;
    const auto* tempo = track.getChildByName ("TEMPO");
    return tempo != nullptr ? tempo->getDoubleAttribute ("Bpm") : 0.0;
}

void fillRecord (const juce::XmlElement& e, TrackRecord& r)
{
    r.path = fileUrlToPath (e.getStringAttribute ("Location"));
    r.title = e.getStringAttribute ("Name");
    r.artist = e.getStringAttribute ("Artist");
    r.album = e.getStringAttribute ("Album");
    r.genre = e.getStringAttribute ("Genre");
    r.comment = e.getStringAttribute ("Comments");
    r.key = e.getStringAttribute ("Tonality");
    r.year = e.getIntAttribute ("Year");
    r.durationSec = e.getDoubleAttribute ("TotalTime");
    r.playCount = juce::jmax (0, e.getIntAttribute ("PlayCount"));
    r.rating = juce::jlimit (0, 5, (e.getIntAttribute ("Rating") + kRatingStep / 2) / kRatingStep);
    r.dateAddedMs = e.hasAttribute ("DateAdded") ? juce::Time::fromISO8601 (e.getStringAttribute ("DateAdded")).toMilliseconds() : 0;
    if (const double bpm = bpmOf (e); bpm > 0.0)
        r.bpm = bpm;
}

using PathToId = std::map<juce::String, juce::String>;

void readTrack (const juce::XmlElement& e, ImportedLibrary& out, ImportReport& report, PathToId& byPath)
{
    ImportedTrack t;
    t.key = e.getStringAttribute ("TrackID");
    t.sampleRate = e.getDoubleAttribute ("SampleRate");
    fillRecord (e, t.record);
    if (t.record.path.isEmpty())
    {
        ++report.tracksSkipped;
        report.note ("skipped track " + t.key + ": no local file location");
        return;
    }
    readMarks (e, t);
    byPath[t.record.path] = t.key;
    out.tracks.push_back (std::move (t));
}

// KeyType 0 refers to TrackID, 1 to the track's Location.
juce::String resolveKey (const juce::XmlElement& ref, bool byLocation, const PathToId& byPath)
{
    const auto key = ref.getStringAttribute ("Key");
    if (! byLocation)
        return key;
    const auto it = byPath.find (fileUrlToPath (key));
    return it == byPath.end() ? juce::String() : it->second;
}

ImportedPlaylist readNode (const juce::XmlElement& node, const PathToId& byPath)
{
    ImportedPlaylist p;
    p.name = node.getStringAttribute ("Name");
    p.isFolder = node.getIntAttribute ("Type") == 0;
    const bool byLocation = node.getIntAttribute ("KeyType") == 1;
    for (auto* child : node.getChildIterator())
    {
        if (child->hasTagName ("NODE"))
            p.children.push_back (readNode (*child, byPath));
        else if (child->hasTagName ("TRACK"))
            p.trackKeys.push_back (resolveKey (*child, byLocation, byPath));
    }
    return p;
}
} // namespace

juce::Result RekordboxImporter::parse (const juce::XmlElement& root, ImportedLibrary& out, ImportReport& report) const
{
    PathToId byPath;
    if (const auto* collection = root.getChildByName ("COLLECTION"))
        for (auto* track : collection->getChildWithTagNameIterator ("TRACK"))
            readTrack (*track, out, report, byPath);
    const auto* playlists = root.getChildByName ("PLAYLISTS");
    const auto* top = playlists != nullptr ? playlists->getChildByName ("NODE") : nullptr;
    if (top != nullptr)                                // the single ROOT node is only a container
        out.playlists = readNode (*top, byPath).children;
    return juce::Result::ok();
}

} // namespace openmixer::library
