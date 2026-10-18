#include "ImportCues.h"

#include <algorithm>
#include <cmath>
#include <optional>
#include <set>

namespace openmixer::library
{
namespace
{
// Tracks which slots and positions are taken so imports never overwrite or duplicate.
class SlotPlan
{
public:
    void markUsed (int slot, std::int64_t position) { slots.insert (slot); positions.insert (position); }

    // The slot to write, or nullopt when local data already covers this cue.
    std::optional<int> claim (int num, std::int64_t position)
    {
        if (positions.count (position) != 0)
            return std::nullopt;
        int slot = num >= 0 ? num : kFirstMemorySlot;
        while (num < 0 && slots.count (slot) != 0)
            ++slot;
        if (slots.count (slot) != 0)
            return std::nullopt;
        markUsed (slot, position);
        return slot;
    }

private:
    std::set<int> slots;
    std::set<std::int64_t> positions;
};

std::int64_t toFrames (double seconds, double rate)
{
    return static_cast<std::int64_t> (std::llround (seconds * rate));
}

bool isValid (const ImportedCue& c, std::int64_t start, std::int64_t end)
{
    const bool loopOk = c.kind != ImportedCue::Kind::Loop || end > start;
    return start >= 0 && loopOk && c.num >= -1;
}

juce::Result loadPlans (CueDao& dao, std::int64_t trackId, SlotPlan& cuePlan, SlotPlan& loopPlan)
{
    juce::Result status = juce::Result::ok();
    for (const auto& c : dao.cues (trackId, &status))
        cuePlan.markUsed (c.slot, c.positionFrames);
    if (status.failed())
        return status;
    for (const auto& l : dao.loops (trackId, &status))
        loopPlan.markUsed (l.slot, l.startFrames);
    return status;
}

juce::Result store (CueDao& dao, std::int64_t trackId, const ImportedCue& c, int slot, std::int64_t start, std::int64_t end)
{
    if (c.kind == ImportedCue::Kind::Loop)
        return dao.setLoop ({ trackId, slot, start, end, c.colourArgb, c.label });
    return dao.setCue ({ trackId, slot, start, c.colourArgb, c.label });
}
} // namespace

juce::Result importCues (CueDao& dao, std::int64_t trackId, const ImportedTrack& track, ImportReport& report)
{
    SlotPlan cuePlan, loopPlan;
    if (auto loaded = loadPlans (dao, trackId, cuePlan, loopPlan); loaded.failed())
        return loaded;
    const double rate = track.sampleRate > 0.0 ? track.sampleRate : kAssumedSampleRate;
    // Hotcues first: a memory cue at the same position must not shadow the hotcue.
    std::vector<ImportedCue> ordered = track.cues;
    std::stable_partition (ordered.begin(), ordered.end(), [] (const ImportedCue& c) { return c.num >= 0; });
    for (const auto& c : ordered)
    {
        const auto start = toFrames (c.startSec, rate), end = toFrames (c.endSec, rate);
        if (! isValid (c, start, end))
        {
            ++report.errors;
            report.note ("invalid cue in " + track.record.path);
            continue;
        }
        auto slot = (c.kind == ImportedCue::Kind::Loop ? loopPlan : cuePlan).claim (c.num, start);
        if (! slot.has_value())
            continue;
        if (auto stored = store (dao, trackId, c, *slot, start, end); stored.failed())
            return stored;
        ++report.cuesImported;
    }
    return juce::Result::ok();
}

} // namespace openmixer::library
