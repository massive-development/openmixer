// OpenMixer StructureAnalyzerTest - onsets, energy curve, sections and mix points on synthetic tracks
#include <JuceHeader.h>
#include "analysis/OnsetPicker.h"
#include "analysis/StructureAnalyzer.h"
#include "AnalysisTestSignals.h"

#include <gtest/gtest.h>

using namespace openmixer::analysis;

namespace
{
constexpr double kBpm = 120.0;

// Adds a click every beat between from and to, scaled by gain.
void addBeats (AudioClip& clip, double from, double to, float gain)
{
    for (double t = from; t < to - 1.0e-9; t += 60.0 / kBpm)
    {
        test::addClick (clip, t, gain);
        test::addKick (clip, t, gain);
    }
}

// intro (quiet beats) 0-16 | main 16-48 | breakdown (soft pad) 48-64 | main 64-96 | outro 96-112
AudioClip arrangedTrack()
{
    auto clip = test::silence (112.0);
    addBeats (clip, 0.0, 16.0, 0.08f);
    addBeats (clip, 16.0, 48.0, 0.8f);
    test::addTone (clip, 220.0, 0.06f, 48.0, 64.0);
    addBeats (clip, 64.0, 96.0, 0.8f);
    addBeats (clip, 96.0, 112.0, 0.08f);
    return clip;
}

std::optional<StructureResult> estimate (const AudioClip& clip)
{
    NullAnalysisContext context;
    return StructureEstimator().estimate (clip, context);
}
} // namespace

TEST (OnsetPicker, FindsEveryClickOnce)
{
    const auto clip = test::clickTrack (kBpm, 10.0, 0.25);
    NullAnalysisContext context;
    const auto env = computeBandOnsets (clip, { 10, 512, 0.1 }, { FrequencyBand {} }, context);
    ASSERT_TRUE (env.has_value());

    const auto onsets = pickOnsets (env->front());
    ASSERT_EQ (onsets.size(), 20u);
    for (size_t i = 0; i < onsets.size(); ++i)
        EXPECT_NEAR (onsets[i], 0.25 + 0.5 * static_cast<double> (i), 0.012) << i;
}

TEST (OnsetPicker, SilenceHasNoOnsets)
{
    const OnsetEnvelope flat { std::vector<float> (500, 0.0f), 86.0, 0.0 };
    EXPECT_TRUE (pickOnsets (flat).empty());
    EXPECT_TRUE (pickOnsets (OnsetEnvelope {}).empty());
}

TEST (StructureAnalyzer, FindsSectionsAndMixPoints)
{
    const auto result = estimate (arrangedTrack());
    ASSERT_TRUE (result.has_value());

    const std::vector<std::pair<double, const char*>> expected {
        { 0.0, "intro" }, { 16.0, "main" }, { 48.0, "breakdown" }, { 64.0, "main" }, { 96.0, "outro" } };
    ASSERT_EQ (result->sections.size(), expected.size());
    for (size_t i = 0; i < expected.size(); ++i)
    {
        EXPECT_NEAR (result->sections[i].startSeconds, expected[i].first, 1.0) << i;
        EXPECT_EQ (result->sections[i].label, expected[i].second) << i;
    }
    EXPECT_DOUBLE_EQ (result->sections.back().endSeconds, 112.0);
    EXPECT_NEAR (result->mixInSeconds, 16.0, 1.0);
    EXPECT_NEAR (result->mixOutSeconds, 96.0, 1.0);
    EXPECT_GT (result->onsetSeconds.size(), 150u);
}

TEST (StructureAnalyzer, EnergyCurveIsRelativeToLoudestWindow)
{
    auto clip = test::silence (10.0);
    test::addTone (clip, 440.0, 0.5f, 0.0, 5.0);
    test::addTone (clip, 440.0, 0.05f, 5.0, 10.0);   // 20 dB quieter
    const auto curve = computeEnergyCurve (clip, 0.5);
    ASSERT_EQ (curve.values.size(), 20u);
    EXPECT_NEAR (curve.values[3], 1.0f, 0.01f);
    EXPECT_NEAR (curve.values[15], 0.5f, 0.02f);
    EXPECT_TRUE (computeEnergyCurve (AudioClip {}).values.empty());
}

TEST (StructureAnalyzer, SteadyTrackIsOneMainSection)
{
    const auto result = estimate (test::clickTrack (kBpm, 40.0, 0.0, 0));
    ASSERT_TRUE (result.has_value());
    ASSERT_EQ (result->sections.size(), 1u);
    EXPECT_EQ (result->sections.front().label, "main");
    EXPECT_DOUBLE_EQ (result->mixInSeconds, 0.0);
    EXPECT_DOUBLE_EQ (result->mixOutSeconds, 40.0);
}

TEST (StructureAnalyzer, PayloadRoundTrips)
{
    StructureAnalyzer analyzer;
    NullAnalysisContext context;
    const auto payload = analyzer.analyze (arrangedTrack(), context);
    ASSERT_TRUE (payload.has_value());
    const auto parsed = StructureResult::fromJson (*payload);
    ASSERT_TRUE (parsed.has_value());
    EXPECT_EQ (parsed->sections.size(), 5u);
    EXPECT_EQ (parsed->sections[2].label, "breakdown");
    EXPECT_EQ (parsed->energy.values.size(), 224u);
    EXPECT_NEAR (parsed->mixOutSeconds, 96.0, 1.0);
    EXPECT_FALSE (StructureResult::fromJson ("[]").has_value());
}
