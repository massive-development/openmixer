// Adds imported cues and loops to a track without disturbing local ones (merge policy in ImportMerge.h).
// Hotcue Num n goes to slot n; memory cues/loops (Num -1) go to the first free slot from 8 up.
// Times are seconds, converted to engine frames with the track's sample rate (44100 when unknown).
#pragma once

#include "CueDao.h"
#include "ImportModel.h"

namespace openmixer::library
{

constexpr int kFirstMemorySlot = 8;
constexpr double kAssumedSampleRate = 44100.0;

// Counts added cues in report.cuesImported and malformed ones in report.errors.
juce::Result importCues (CueDao& dao, std::int64_t trackId, const ImportedTrack& track, ImportReport& report);

} // namespace openmixer::library
