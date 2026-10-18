#include "PrepResults.h"

#include "analysis/BpmEstimator.h"
#include "analysis/KeyEstimator.h"

namespace openmixer::library
{
namespace
{
std::optional<double> parseBpm (const std::map<juce::String, juce::String>& results)
{
    const auto it = results.find ("bpm");
    if (it == results.end())
        return std::nullopt;
    const auto parsed = analysis::BpmResult::fromJson (it->second);
    return parsed && parsed->bpm > 0.0 ? std::optional<double> (parsed->bpm) : std::nullopt;
}

std::optional<analysis::MusicalKey> parseKey (const std::map<juce::String, juce::String>& results)
{
    const auto it = results.find ("key");
    if (it == results.end())
        return std::nullopt;
    const auto parsed = analysis::KeyResult::fromJson (it->second);
    return parsed && parsed->tonal ? std::optional<analysis::MusicalKey> (parsed->key) : std::nullopt;
}
} // namespace

juce::Result applyPrepResults (TrackDao& tracks, std::int64_t trackId, const std::map<juce::String, juce::String>& results)
{
    juce::Result read = juce::Result::ok();
    const auto track = tracks.findById (trackId, &read);
    if (read.failed())
        return read;
    if (! track)
        return juce::Result::fail ("track no longer exists");

    const auto bpm = parseBpm (results);
    const auto key = parseKey (results);
    if (! bpm && ! key)
        return juce::Result::ok();

    return tracks.updateAnalysis (trackId, bpm, key);   // narrow write: a whole-row update races the scan connection
}

} // namespace openmixer::library
