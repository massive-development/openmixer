// Onset times from an onset envelope: local maxima above an adaptive (local mean) threshold
// and a global floor, at least minGapSeconds apart.
#pragma once

#include "OnsetEnvelope.h"

namespace openmixer::analysis
{

struct OnsetPickOptions
{
    double minGapSeconds = 0.05;
    double localWindowSeconds = 0.1;   // radius of the adaptive threshold window
    float localFactor = 1.5f;          // peak must exceed this x the local mean
    float globalFraction = 0.1f;       // ... and this fraction of the envelope maximum
};

std::vector<double> pickOnsets (const OnsetEnvelope& env, const OnsetPickOptions& options = {});

} // namespace openmixer::analysis
