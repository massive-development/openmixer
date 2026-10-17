#include "BpmEstimator.h"

#include "OnsetEnvelope.h"
#include "TempoSearch.h"

#include <cmath>

namespace openmixer::analysis
{
namespace
{
constexpr double kMinSeconds = 4.0;
constexpr double kOnsetLatencySeconds = -0.002;   // flux peaks ~2 ms before the onset (calibrated on clicks)
const FrequencyBand kFullBand { 30.0, 16000.0 };
const FrequencyBand kKickBand { 30.0, 200.0 };

double foldIntoRange (double bpm, double lo, double hi) noexcept
{
    while (bpm > 0.0 && bpm < lo)
        bpm *= 2.0;
    while (bpm >= hi)
        bpm *= 0.5;
    return bpm;
}

double meanOf (const std::vector<float>& v)
{
    double sum = 0.0;
    for (auto x : v)
        sum += x;
    return v.empty() ? 0.0 : sum / static_cast<double> (v.size());
}

// Mean on-beat onset vs the overall mean: 1 for a clean pulse, 0 when beats are no louder than average.
double confidenceOf (const OnsetEnvelope& env, const BeatPhase& beats)
{
    return beats.score > 0.0 ? juce::jlimit (0.0, 1.0, 1.0 - meanOf (env.values) / beats.score) : 0.0;
}

// First downbeat at or after the start of the clip, in source frames.
double firstDownbeatFrame (const OnsetEnvelope& env, const BeatPhase& beats, int offset, int beatsPerBar,
                           double sampleRate)
{
    const auto index = beats.phase + offset * periodFrames (env, beats.bpm);
    const auto seconds = env.timeOf (index) - kOnsetLatencySeconds;
    const auto barSeconds = beatsPerBar * 60.0 / beats.bpm;
    return (seconds - barSeconds * std::floor (seconds / barSeconds)) * sampleRate;
}
} // namespace

std::optional<BpmResult> BpmEstimator::estimate (const AudioClip& clip, AnalysisContext& context) const
{
    if (clip.getDurationSeconds() < kMinSeconds)
        return std::nullopt;

    SubProgressContext onsetStep (context, 0.0f, 0.8f);
    const auto bands = computeBandOnsets (clip, {}, { kFullBand, kKickBand }, onsetStep);
    if (! bands)
        return std::nullopt;
    const auto& full = (*bands)[0];

    const auto coarse = estimateCoarseBpm (full, options.minBpm, options.maxBpm);
    auto beats = refineBeatPhase (full, foldIntoRange (coarse, options.minBpm, options.maxBpm));
    context.setProgress (0.95f);
    if (beats.bpm <= 0.0 || context.isCancelled())
        return std::nullopt;

    const auto offset = findDownbeatOffset ((*bands)[1], beats, options.beatsPerBar);
    BpmResult result;
    result.bpm = beats.bpm;
    result.confidence = confidenceOf (full, beats);
    result.grid = BeatGridModel::constant (beats.bpm, clip.sampleRate,
                                           firstDownbeatFrame (full, beats, offset, options.beatsPerBar, clip.sampleRate),
                                           options.beatsPerBar);
    context.setProgress (1.0f);
    return result;
}

juce::String BpmResult::toJson() const
{
    auto obj = std::make_unique<juce::DynamicObject>();
    obj->setProperty ("bpm", bpm);
    obj->setProperty ("confidence", confidence);
    if (grid.isValid())
        obj->setProperty ("grid", grid.toVar());
    return juce::JSON::toString (juce::var (obj.release()), true);
}

std::optional<BpmResult> BpmResult::fromJson (const juce::String& json)
{
    const auto v = juce::JSON::parse (json);
    if (! v.isObject() || ! v.hasProperty ("bpm"))
        return std::nullopt;

    BpmResult r;
    r.bpm = static_cast<double> (v["bpm"]);
    r.confidence = static_cast<double> (v["confidence"]);
    if (v.hasProperty ("grid"))
    {
        auto grid = BeatGridModel::fromVar (v["grid"]);
        if (! grid)
            return std::nullopt;
        r.grid = *grid;
    }
    return r;
}

std::optional<juce::String> BpmAnalyzer::analyze (const AudioClip& clip, AnalysisContext& context) const
{
    auto result = estimator.estimate (clip, context);
    if (context.isCancelled())
        return std::nullopt;
    return result.value_or (BpmResult {}).toJson();   // no pulse is a valid result: bpm 0, no grid
}

} // namespace openmixer::analysis
