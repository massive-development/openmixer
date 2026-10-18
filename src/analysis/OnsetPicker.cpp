#include "OnsetPicker.h"

#include <algorithm>

namespace openmixer::analysis
{
namespace
{
bool isLocalMax (const std::vector<float>& v, size_t i, size_t radius)
{
    const auto lo = i > radius ? i - radius : 0, hi = std::min (v.size() - 1, i + radius);
    for (auto j = lo; j <= hi; ++j)
        if (v[j] > v[i] || (v[j] == v[i] && j < i))
            return false;
    return true;
}

float localMean (const std::vector<float>& v, size_t i, size_t radius)
{
    const auto lo = i > radius ? i - radius : 0, hi = std::min (v.size() - 1, i + radius);
    double sum = 0.0;
    for (auto j = lo; j <= hi; ++j)
        sum += v[j];
    return static_cast<float> (sum / static_cast<double> (hi - lo + 1));
}
} // namespace

std::vector<double> pickOnsets (const OnsetEnvelope& env, const OnsetPickOptions& options)
{
    std::vector<double> onsets;
    const auto& v = env.values;
    if (v.empty() || env.frameRate <= 0.0)
        return onsets;

    const auto floor = options.globalFraction * *std::max_element (v.begin(), v.end());
    const auto gap = static_cast<size_t> (std::max (1.0, options.minGapSeconds * env.frameRate));
    const auto window = static_cast<size_t> (std::max (1.0, options.localWindowSeconds * env.frameRate));
    for (size_t i = 0; i < v.size(); ++i)
    {
        if (v[i] <= floor || ! isLocalMax (v, i, gap) || v[i] < options.localFactor * localMean (v, i, window))
            continue;
        onsets.push_back (env.timeOf (static_cast<double> (i)));
    }
    return onsets;
}

} // namespace openmixer::analysis
