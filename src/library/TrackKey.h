// Keeps TrackRecord key and camelot consistent; shared by importers, merge and prep results.
#pragma once

#include "TrackRecord.h"
#include "analysis/MusicalKey.h"

namespace openmixer::library
{

// Sets key (canonical name) and camelot from a parsed key.
void setKey (TrackRecord& track, const analysis::MusicalKey& key);

// Derives the missing/canonical key and camelot from whichever one is present (name or Camelot code).
// Unparseable text is left untouched.
void normaliseKey (TrackRecord& track);

} // namespace openmixer::library
