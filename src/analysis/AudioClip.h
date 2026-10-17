// Decoded mono audio handed to analysers; immutable once a job starts.
#pragma once

#include <cstddef>
#include <vector>

namespace openmixer::analysis
{

struct AudioClip
{
    std::vector<float> samples;   // mono, [-1, 1]
    double sampleRate = 44100.0;

    double getDurationSeconds() const noexcept
    {
        return sampleRate > 0.0 ? static_cast<double> (samples.size()) / sampleRate : 0.0;
    }
};

} // namespace openmixer::analysis
