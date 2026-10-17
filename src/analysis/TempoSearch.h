// Tempo/phase search steps on an onset envelope, split out of BpmEstimator so each is testable.
// Indices are envelope frames (fractional); periods are frames per beat.
#pragma once

#include "OnsetEnvelope.h"

namespace openmixer::analysis
{

struct BeatPhase
{
    double bpm = 0.0;
    double phase = 0.0;   // envelope index of a beat, in [0, period)
    double score = 0.0;   // mean envelope value on the beat positions
};

// Autocorrelation peak within [minBpm, maxBpm]; 0 when the envelope is too short or flat.
double estimateCoarseBpm (const OnsetEnvelope& env, double minBpm, double maxBpm);

// Joint fine search of tempo (within +-2% of the coarse value) and beat phase.
BeatPhase refineBeatPhase (const OnsetEnvelope& env, double coarseBpm);

// Least-squares line through the onset peak nearest each beat: sub-frame tempo and phase.
BeatPhase fitToOnsets (const OnsetEnvelope& env, const BeatPhase& beats);

// Which beat of the bar (0..beatsPerBar-1, counted from `phase`) carries the most low-band energy.
int findDownbeatOffset (const OnsetEnvelope& lowBand, const BeatPhase& beats, int beatsPerBar);

double periodFrames (const OnsetEnvelope& env, double bpm) noexcept;

} // namespace openmixer::analysis
