#include "ImportMerge.h"

#include "TrackKey.h"

namespace openmixer::library
{
namespace
{
bool fillText (juce::String& local, const juce::String& imported)
{
    if (local.isNotEmpty() || imported.isEmpty())
        return false;
    local = imported;
    return true;
}

template <typename T>
bool fillDefault (T& local, const T& imported)
{
    if (local != T{} || imported == T{})
        return false;
    local = imported;
    return true;
}

bool fillOptional (std::optional<double>& local, const std::optional<double>& imported)
{
    if (local.has_value() || ! imported.has_value())
        return false;
    local = imported;
    return true;
}

bool completeKey (TrackRecord& local)
{
    const auto before = std::make_pair (local.key, local.camelot);
    normaliseKey (local);   // why: a filled key needs its camelot or key: search misses it
    return before != std::make_pair (local.key, local.camelot);
}
} // namespace

bool mergeTrackRecord (TrackRecord& local, const TrackRecord& imported)
{
    bool changed = false;
    for (auto [l, i] : { std::pair<juce::String*, const juce::String*> { &local.title, &imported.title },
                         { &local.artist, &imported.artist }, { &local.album, &imported.album },
                         { &local.genre, &imported.genre }, { &local.comment, &imported.comment },
                         { &local.key, &imported.key } })
        changed |= fillText (*l, *i);
    changed |= fillDefault (local.year, imported.year);
    changed |= fillDefault (local.durationSec, imported.durationSec);
    changed |= fillDefault (local.rating, imported.rating);
    changed |= fillDefault (local.playCount, imported.playCount);
    changed |= fillOptional (local.bpm, imported.bpm);
    changed |= completeKey (local);
    if (! local.lastPlayedMs.has_value() && imported.lastPlayedMs.has_value())
    {
        local.lastPlayedMs = imported.lastPlayedMs;
        changed = true;
    }
    return changed;
}

} // namespace openmixer::library
