#include "KeyEstimator.h"

#include <algorithm>
#include <cmath>
#include <numeric>

namespace openmixer::analysis
{
namespace
{
// Project templates (not taken from published profiles): tonic strongest, then the
// tonic-triad tones, then the remaining scale tones; minor gives both 6ths and 7ths partial weight.
constexpr Chroma kMajorTemplate { 1.0, 0.1, 0.5, 0.1, 0.8, 0.5, 0.1, 0.85, 0.1, 0.5, 0.1, 0.45 };
constexpr Chroma kMinorTemplate { 1.0, 0.1, 0.5, 0.8, 0.1, 0.5, 0.1, 0.85, 0.4, 0.2, 0.4, 0.35 };
constexpr double kMinPeak = 1.0e-6;

double correlation (const Chroma& chroma, const Chroma& keyTemplate, int tonic)
{
    const auto meanA = std::accumulate (chroma.begin(), chroma.end(), 0.0) / 12.0;
    const auto meanB = std::accumulate (keyTemplate.begin(), keyTemplate.end(), 0.0) / 12.0;
    double ab = 0.0, aa = 0.0, bb = 0.0;
    for (size_t pc = 0; pc < 12; ++pc)
    {
        const auto a = chroma[pc] - meanA;
        const auto b = keyTemplate[static_cast<size_t> ((static_cast<int> (pc) - tonic + 12) % 12)] - meanB;
        ab += a * b; aa += a * a; bb += b * b;
    }
    return aa > 0.0 && bb > 0.0 ? ab / std::sqrt (aa * bb) : 0.0;
}

Chroma normalised (const Chroma& c)
{
    const auto peak = *std::max_element (c.begin(), c.end());
    Chroma out {};
    for (size_t i = 0; i < 12; ++i)
        out[i] = peak > 0.0 ? c[i] / peak : 0.0;
    return out;
}

juce::var chromaToVar (const Chroma& c)
{
    juce::Array<juce::var> list;
    for (auto v : c)
        list.add (v);
    return list;
}
} // namespace

std::optional<KeyResult> KeyEstimator::classify (const Chroma& chroma)
{
    if (*std::max_element (chroma.begin(), chroma.end()) < kMinPeak)
        return std::nullopt;

    double best = -2.0, second = -2.0;
    MusicalKey bestKey;
    for (int tonic = 0; tonic < 12; ++tonic)
        for (const bool minor : { false, true })
        {
            const auto score = correlation (chroma, minor ? kMinorTemplate : kMajorTemplate, tonic);
            if (score > best)
            {
                second = best;
                best = score;
                bestKey = { tonic, minor };
            }
            else
                second = std::max (second, score);
        }
    return KeyResult { bestKey, juce::jlimit (0.0, 1.0, (best - second) * 5.0), normalised (chroma), true };
}

std::optional<KeyResult> KeyEstimator::estimate (const AudioClip& clip, AnalysisContext& context) const
{
    const auto chroma = computeChroma (clip, options, context);
    return chroma ? classify (*chroma) : std::nullopt;
}

juce::String KeyResult::toJson() const
{
    auto obj = std::make_unique<juce::DynamicObject>();
    if (! tonal)
    {
        obj->setProperty ("none", true);
        return juce::JSON::toString (juce::var (obj.release()), true);
    }
    obj->setProperty ("tonic", key.tonic);
    obj->setProperty ("mode", key.minor ? "minor" : "major");
    obj->setProperty ("name", key.getName());
    obj->setProperty ("camelot", key.getCamelot());
    obj->setProperty ("confidence", confidence);
    obj->setProperty ("chroma", chromaToVar (chroma));
    return juce::JSON::toString (juce::var (obj.release()), true);
}

std::optional<KeyResult> KeyResult::fromJson (const juce::String& json)
{
    const auto v = juce::JSON::parse (json);
    if (! v.isObject())
        return std::nullopt;
    if (static_cast<bool> (v["none"]))
        return KeyResult { {}, 0.0, {}, false };

    const auto tonic = static_cast<int> (v["tonic"]);
    if (! v.hasProperty ("tonic") || tonic < 0 || tonic > 11)
        return std::nullopt;

    KeyResult r { { tonic, v["mode"].toString() == "minor" }, static_cast<double> (v["confidence"]), {}, true };
    if (const auto* list = v["chroma"].getArray(); list != nullptr && list->size() == 12)
        for (int i = 0; i < 12; ++i)
            r.chroma[static_cast<size_t> (i)] = static_cast<double> ((*list)[i]);
    return r;
}

std::optional<juce::String> KeyAnalyzer::analyze (const AudioClip& clip, AnalysisContext& context) const
{
    const auto result = estimator.estimate (clip, context);
    if (context.isCancelled())
        return std::nullopt;
    return result.value_or (KeyResult { {}, 0.0, {}, false }).toJson();   // atonal/silent is a valid answer
}

} // namespace openmixer::analysis
