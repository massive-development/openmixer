#include "TrackKey.h"

namespace openmixer::library
{
namespace
{
std::optional<analysis::MusicalKey> parse (const juce::String& text)
{
    const auto trimmed = text.trim();
    if (trimmed.isEmpty())
        return std::nullopt;
    if (auto k = analysis::MusicalKey::fromName (trimmed))
        return k;
    return analysis::MusicalKey::fromCamelot (trimmed);
}
} // namespace

void setKey (TrackRecord& track, const analysis::MusicalKey& key)
{
    track.key = key.getName();
    track.camelot = key.getCamelot();
}

void normaliseKey (TrackRecord& track)
{
    if (auto k = parse (track.key.isNotEmpty() ? track.key : track.camelot))
        setKey (track, *k);
}

} // namespace openmixer::library
