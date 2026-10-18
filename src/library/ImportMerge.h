// Merge policy for tracks that already exist locally (matched by path). Local data wins:
//  - text, year, duration, BPM, key, last-played: imported value fills a field only when the local one is empty/unset;
//  - rating and play count: imported value is taken only when the local one is still the default (0);
//  - id, path, content hash, analysis, date added are never touched;
//  - cues and loops: an imported one is added only into a slot that is free locally and not already
//    present at the same position (so re-importing the same file adds nothing).
#pragma once

#include "TrackRecord.h"

namespace openmixer::library
{

// Applies the policy to `local`; true when anything changed.
bool mergeTrackRecord (TrackRecord& local, const TrackRecord& imported);

} // namespace openmixer::library
