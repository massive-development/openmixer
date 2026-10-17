#include "StructureAnalyzer.h"

#include "OnsetPicker.h"

#include <algorithm>

namespace openmixer::analysis
{
namespace
{
juce::var sectionToVar (const Section& s)
{
    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("start", s.startSeconds);
    obj->setProperty ("end", s.endSeconds);
    obj->setProperty ("energy", s.energy);
    obj->setProperty ("label", s.label);
    return juce::var (obj.release());
}

template <typename T>
juce::var listToVar (const std::vector<T>& values)
{
    juce::Array<juce::var> list;
    for (auto v : values)
        list.add (static_cast<double> (v));
    return list;
}

template <typename T>
std::vector<T> listFromVar (const juce::var& v)
{
    std::vector<T> out;
    if (const auto* list = v.getArray())
        for (const auto& item : *list)
            out.push_back (static_cast<T> (static_cast<double> (item)));
    return out;
}

void setMixPoints (StructureResult& r, double duration)
{
    r.mixInSeconds = 0.0;
    r.mixOutSeconds = duration;
    const auto isMain = [] (const Section& s) { return s.label == "main"; };
    const auto first = std::find_if (r.sections.begin(), r.sections.end(), isMain);
    const auto last = std::find_if (r.sections.rbegin(), r.sections.rend(), isMain);
    if (first != r.sections.end())
        r.mixInSeconds = first->startSeconds;
    if (last != r.sections.rend())
        r.mixOutSeconds = last->endSeconds;
}
} // namespace

std::optional<StructureResult> StructureEstimator::estimate (const AudioClip& clip, AnalysisContext& context) const
{
    SubProgressContext onsetStep (context, 0.0f, 0.9f);
    const auto onsets = computeBandOnsets (clip, { 10, 512, 0.1 }, { FrequencyBand {} }, onsetStep);
    if (! onsets)
        return std::nullopt;

    StructureResult r;
    r.onsetSeconds = pickOnsets (onsets->front());
    r.energy = computeEnergyCurve (clip);
    r.sections = segmentSections (r.energy, options);
    const auto duration = clip.getDurationSeconds();
    if (! r.sections.empty())
        r.sections.back().endSeconds = std::min (r.sections.back().endSeconds, duration);
    setMixPoints (r, duration);
    context.setProgress (1.0f);
    return r;
}

juce::String StructureResult::toJson() const
{
    juce::Array<juce::var> sectionList;
    for (const auto& s : sections)
        sectionList.add (sectionToVar (s));

    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("version", 1);
    obj->setProperty ("onsets", listToVar (onsetSeconds));
    obj->setProperty ("energyHop", energy.hopSeconds);
    obj->setProperty ("energy", listToVar (energy.values));
    obj->setProperty ("sections", sectionList);
    obj->setProperty ("mixIn", mixInSeconds);
    obj->setProperty ("mixOut", mixOutSeconds);
    return juce::JSON::toString (juce::var (obj.release()), true);
}

std::optional<StructureResult> StructureResult::fromJson (const juce::String& json)
{
    const auto v = juce::JSON::parse (json);
    if (static_cast<int> (v["version"]) != 1)
        return std::nullopt;

    StructureResult r;
    r.onsetSeconds = listFromVar<double> (v["onsets"]);
    r.energy = { static_cast<double> (v["energyHop"]), listFromVar<float> (v["energy"]) };
    if (const auto* list = v["sections"].getArray())
        for (const auto& s : *list)
            r.sections.push_back ({ static_cast<double> (s["start"]), static_cast<double> (s["end"]),
                                    static_cast<float> (static_cast<double> (s["energy"])), s["label"].toString() });
    r.mixInSeconds = static_cast<double> (v["mixIn"]);
    r.mixOutSeconds = static_cast<double> (v["mixOut"]);
    return r;
}

std::optional<juce::String> StructureAnalyzer::analyze (const AudioClip& clip, AnalysisContext& context) const
{
    const auto result = estimator.estimate (clip, context);
    if (! result || context.isCancelled())
        return std::nullopt;
    return result->toJson();
}

} // namespace openmixer::analysis
