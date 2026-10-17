// Loudness curve and section boundaries for structure analysis (feeds Automix). Sections are cut
// where the smoothed energy before and after a point differs most, then labelled by loudness.
#pragma once

#include "AudioClip.h"

#include <juce_core/juce_core.h>

#include <vector>

namespace openmixer::analysis
{

struct EnergyCurve
{
    double hopSeconds = 0.5;
    std::vector<float> values;   // 0..1: RMS in dB mapped over the 40 dB below the clip's loudest window
};

struct Section
{
    double startSeconds = 0.0, endSeconds = 0.0;
    float energy = 0.0f;   // mean of the curve over the section
    juce::String label;    // intro, main, breakdown, outro
};

struct SegmentOptions
{
    double windowSeconds = 8.0;       // energy compared over this span either side of a boundary
    double minSectionSeconds = 8.0;
    float minChange = 0.15f;          // energy step needed for a boundary
    double mainWithinDb = 6.0;        // sections this close to the loud level count as main
};

EnergyCurve computeEnergyCurve (const AudioClip& clip, double hopSeconds = 0.5);
std::vector<Section> segmentSections (const EnergyCurve& curve, const SegmentOptions& options = {});

} // namespace openmixer::analysis
