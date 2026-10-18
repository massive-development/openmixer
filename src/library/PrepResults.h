// Turns finished analysis payloads into track metadata. Runs on the caller's thread with the
// caller's TrackDao. The schema has no manual/locked flag for bpm or key, so analysis overwrites.
#pragma once

#include "TrackDao.h"

#include <map>

namespace openmixer::library
{

// Writes bpm and key/camelot found in `results` (analyser id -> JSON). Missing or unusable
// entries leave the stored value alone; a track that no longer exists or a failed read is an error.
juce::Result applyPrepResults (TrackDao& tracks, std::int64_t trackId, const std::map<juce::String, juce::String>& results);

} // namespace openmixer::library
