#include "CueDao.h"

namespace openmixer::library
{

juce::Result CueDao::setCue (const CuePoint& c)
{
    auto s = db.prepare ("INSERT OR REPLACE INTO cues (track_id, slot, position_frames, colour_argb, label)"
                         " VALUES (?1,?2,?3,?4,?5);");
    s.bindAll (c.trackId, c.slot, c.positionFrames, static_cast<std::int64_t> (c.colourArgb), c.label);
    return s.run();
}

juce::Result CueDao::clearCue (std::int64_t trackId, int slot)
{
    auto s = db.prepare ("DELETE FROM cues WHERE track_id=?1 AND slot=?2;");
    s.bindAll (trackId, slot);
    return s.run();
}

std::vector<CuePoint> CueDao::cues (std::int64_t trackId)
{
    auto s = db.prepare ("SELECT slot, position_frames, colour_argb, label FROM cues"
                         " WHERE track_id=?1 ORDER BY slot;");
    s.bindAll (trackId);

    std::vector<CuePoint> out;
    while (s.nextRow())
        out.push_back ({ trackId, s.integer (0), s.int64 (1), static_cast<std::uint32_t> (s.int64 (2)), s.text (3) });
    return out;
}

juce::Result CueDao::setLoop (const SavedLoop& l)
{
    auto s = db.prepare ("INSERT OR REPLACE INTO loops (track_id, slot, start_frames, end_frames, colour_argb, label)"
                         " VALUES (?1,?2,?3,?4,?5,?6);");
    s.bindAll (l.trackId, l.slot, l.startFrames, l.endFrames, static_cast<std::int64_t> (l.colourArgb), l.label);
    return s.run();
}

juce::Result CueDao::clearLoop (std::int64_t trackId, int slot)
{
    auto s = db.prepare ("DELETE FROM loops WHERE track_id=?1 AND slot=?2;");
    s.bindAll (trackId, slot);
    return s.run();
}

std::vector<SavedLoop> CueDao::loops (std::int64_t trackId)
{
    auto s = db.prepare ("SELECT slot, start_frames, end_frames, colour_argb, label FROM loops"
                         " WHERE track_id=?1 ORDER BY slot;");
    s.bindAll (trackId);

    std::vector<SavedLoop> out;
    while (s.nextRow())
        out.push_back ({ trackId, s.integer (0), s.int64 (1), s.int64 (2), static_cast<std::uint32_t> (s.int64 (3)), s.text (4) });
    return out;
}

} // namespace openmixer::library
