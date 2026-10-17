#include "Segmenter.h"

#include <algorithm>
#include <cmath>
#include <numeric>

namespace openmixer::analysis
{
namespace
{
constexpr double kRangeDb = 40.0;

class PrefixMean
{
public:
    explicit PrefixMean (const std::vector<float>& v) : sums (v.size() + 1, 0.0)
    {
        for (size_t i = 0; i < v.size(); ++i)
            sums[i + 1] = sums[i] + v[i];
    }

    double mean (size_t from, size_t to) const noexcept   // [from, to)
    {
        return to > from ? (sums[to] - sums[from]) / static_cast<double> (to - from) : 0.0;
    }

private:
    std::vector<double> sums;
};

std::vector<double> noveltyCurve (const EnergyCurve& curve, size_t span)
{
    const PrefixMean prefix (curve.values);
    const auto n = curve.values.size();
    std::vector<double> novelty (n, 0.0);
    for (size_t i = 1; i < n; ++i)
        novelty[i] = std::abs (prefix.mean (i, std::min (n, i + span)) - prefix.mean (i > span ? i - span : 0, i));
    return novelty;
}

// Strongest change points first, each at least minGap from the others and from both ends.
std::vector<size_t> pickBoundaries (const std::vector<double>& novelty, size_t minGap, double minChange)
{
    std::vector<size_t> order (novelty.size());
    std::iota (order.begin(), order.end(), size_t { 0 });
    std::sort (order.begin(), order.end(), [&] (size_t a, size_t b) { return novelty[a] > novelty[b]; });

    std::vector<size_t> picked;
    for (auto i : order)
    {
        if (novelty[i] < minChange)
            break;
        const auto farFrom = [&] (size_t j) { return (i > j ? i - j : j - i) >= minGap; };
        if (farFrom (0) && farFrom (novelty.size()) && std::all_of (picked.begin(), picked.end(), farFrom))
            picked.push_back (i);
    }
    std::sort (picked.begin(), picked.end());
    return picked;
}

// The loudest section sets the reference, so steady tracks are a single main section.
void labelSections (std::vector<Section>& sections, double mainWithinDb)
{
    float loud = 0.0f;
    for (const auto& s : sections)
        loud = std::max (loud, s.energy);
    for (auto& s : sections)
        s.label = s.energy >= loud - static_cast<float> (mainWithinDb / kRangeDb) ? "main" : "breakdown";
    if (sections.size() < 2)
        return;
    if (sections.front().label != "main")
        sections.front().label = "intro";
    if (sections.back().label != "main")
        sections.back().label = "outro";
}
} // namespace

EnergyCurve computeEnergyCurve (const AudioClip& clip, double hopSeconds)
{
    EnergyCurve curve { hopSeconds, {} };
    const auto hop = static_cast<size_t> (std::max (1.0, hopSeconds * clip.sampleRate));
    std::vector<double> db;
    for (size_t start = 0; start < clip.samples.size(); start += hop)
    {
        const auto end = std::min (clip.samples.size(), start + hop);
        double sum = 0.0;
        for (auto i = start; i < end; ++i)
            sum += static_cast<double> (clip.samples[i]) * clip.samples[i];
        db.push_back (10.0 * std::log10 (sum / static_cast<double> (end - start) + 1.0e-12));
    }
    if (db.empty())
        return curve;
    const auto top = *std::max_element (db.begin(), db.end());
    for (auto d : db)
        curve.values.push_back (static_cast<float> (juce::jlimit (0.0, 1.0, (d - (top - kRangeDb)) / kRangeDb)));
    return curve;
}

std::vector<Section> segmentSections (const EnergyCurve& curve, const SegmentOptions& options)
{
    std::vector<Section> sections;
    if (curve.values.empty() || curve.hopSeconds <= 0.0)
        return sections;

    const auto span = static_cast<size_t> (std::max (1.0, std::round (options.windowSeconds / curve.hopSeconds)));
    const auto minGap = static_cast<size_t> (std::max (1.0, std::round (options.minSectionSeconds / curve.hopSeconds)));
    auto cuts = pickBoundaries (noveltyCurve (curve, span), minGap, options.minChange);
    cuts.insert (cuts.begin(), 0);
    cuts.push_back (curve.values.size());

    const PrefixMean prefix (curve.values);
    for (size_t s = 0; s + 1 < cuts.size(); ++s)
        sections.push_back ({ static_cast<double> (cuts[s]) * curve.hopSeconds, static_cast<double> (cuts[s + 1]) * curve.hopSeconds,
                              static_cast<float> (prefix.mean (cuts[s], cuts[s + 1])), {} });
    labelSections (sections, options.mainWithinDb);
    return sections;
}

} // namespace openmixer::analysis
