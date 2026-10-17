// OpenMixer BpmEstimatorTest - tempo, phase and downbeat on synthetic click tracks; grid model edits
#include <JuceHeader.h>
#include "analysis/AnalysedBeatGrid.h"
#include "analysis/AnalysisService.h"
#include "analysis/BpmEstimator.h"
#include "engine/Quantizer.h"
#include "AnalysisTestSignals.h"

#include <gtest/gtest.h>

#include <cmath>

using namespace openmixer::analysis;

namespace
{
constexpr double kRate = 44100.0;
constexpr double kBeatToleranceSeconds = 0.003;

std::optional<BpmResult> estimate (const AudioClip& clip, BpmOptions options = {})
{
    NullAnalysisContext context;
    return BpmEstimator (options).estimate (clip, context);
}

// Distance (seconds) from time t to the nearest grid beat.
double gridError (const BeatGridModel& grid, double seconds)
{
    const auto beat = grid.beatAtFrame (seconds * grid.getSampleRate());
    const auto nearest = grid.frameAtBeat (std::round (beat)) / grid.getSampleRate();
    return std::abs (nearest - seconds);
}

double barPosition (const BeatGridModel& grid, double seconds)
{
    const auto beat = std::round (grid.beatAtFrame (seconds * grid.getSampleRate()));
    const auto bar = static_cast<double> (grid.getBeatsPerBar());
    return beat - bar * std::floor (beat / bar);
}

class BpmAccuracy : public ::testing::TestWithParam<double> {};
} // namespace

TEST_P (BpmAccuracy, ClickTrackTempoAndPhase)
{
    const auto bpm = GetParam();
    const auto first = 0.37;
    const auto result = estimate (test::clickTrack (bpm, 30.0, first));
    ASSERT_TRUE (result.has_value());

    EXPECT_NEAR (result->bpm, bpm, 0.05);
    EXPECT_GT (result->confidence, 0.5);
    ASSERT_TRUE (result->grid.isValid());
    for (double t = first; t < 30.0; t += 60.0 / bpm * 7.0)
        EXPECT_LT (gridError (result->grid, t), kBeatToleranceSeconds) << "t=" << t;
}

INSTANTIATE_TEST_SUITE_P (Tempos, BpmAccuracy, ::testing::Values (90.5, 120.0, 128.0, 140.0, 174.0));

TEST (BpmEstimator, FindsAccentedDownbeat)
{
    const auto bpm = 124.0, first = 0.2;
    const auto result = estimate (test::clickTrack (bpm, 30.0, first, 2));
    ASSERT_TRUE (result.has_value());

    const auto downbeat = first + 2.0 * 60.0 / bpm;
    EXPECT_EQ (barPosition (result->grid, downbeat), 0.0);
    EXPECT_EQ (barPosition (result->grid, downbeat + 60.0 / bpm), 1.0);
    const auto firstGridDownbeat = result->grid.frameAtBeat (0.0) / kRate;
    EXPECT_GE (firstGridDownbeat, 0.0);
    EXPECT_LT (firstGridDownbeat, 4.0 * 60.0 / bpm);
}

TEST (BpmEstimator, ToleratesNoise)
{
    auto clip = test::clickTrack (126.0, 30.0, 0.5, 0);
    test::addNoise (clip, 0.05f);
    const auto result = estimate (clip);
    ASSERT_TRUE (result.has_value());
    EXPECT_NEAR (result->bpm, 126.0, 0.05);
}

TEST (BpmEstimator, FoldsTempoIntoRange)
{
    const auto fast = estimate (test::clickTrack (200.0, 30.0, 0.1));
    ASSERT_TRUE (fast.has_value());
    EXPECT_NEAR (fast->bpm, 100.0, 0.05);

    const auto dnb = estimate (test::clickTrack (172.0, 30.0, 0.1), { 60.0, 120.0, 4 });
    ASSERT_TRUE (dnb.has_value());
    EXPECT_NEAR (dnb->bpm, 86.0, 0.05);
}

TEST (BpmEstimator, NoPulseOrTooShortGivesNothing)
{
    EXPECT_FALSE (estimate (test::silence (10.0)).has_value());
    EXPECT_FALSE (estimate (test::clickTrack (120.0, 2.0, 0.0)).has_value());
}

TEST (BpmAnalyzer, PayloadRoundTripsAndSilenceIsValid)
{
    BpmAnalyzer analyzer;
    NullAnalysisContext context;
    const auto payload = analyzer.analyze (test::clickTrack (128.0, 20.0, 0.1), context);
    ASSERT_TRUE (payload.has_value());
    const auto parsed = BpmResult::fromJson (*payload);
    ASSERT_TRUE (parsed.has_value());
    EXPECT_NEAR (parsed->bpm, 128.0, 0.05);
    EXPECT_TRUE (parsed->grid.isValid());

    const auto quiet = BpmResult::fromJson (analyzer.analyze (test::silence (10.0), context).value_or (""));
    ASSERT_TRUE (quiet.has_value());
    EXPECT_EQ (quiet->bpm, 0.0);
    EXPECT_FALSE (quiet->grid.isValid());
    EXPECT_FALSE (BpmResult::fromJson ("not json").has_value());
}

TEST (BpmAnalyzerSlow, RunsThroughServiceAndFeedsEngineGrid)
{
    AnalyzerRegistry registry;
    ASSERT_TRUE (registry.add (std::make_shared<BpmAnalyzer>()).wasOk());
    AnalysisService service (registry, nullptr);
    auto job = service.submit (std::make_unique<MemoryClipSource> (test::clickTrack (128.0, 20.0, 0.0), "clicks"), { "bpm" });
    ASSERT_TRUE (job->waitUntilFinished (60000));
    ASSERT_EQ (job->getStatus(), JobStatus::Done);

    const auto result = BpmResult::fromJson (job->getResults().at ("bpm"));
    ASSERT_TRUE (result.has_value());
    const auto grid = makeEngineGrid (result->grid);
    ASSERT_NE (grid, nullptr);
    const auto beatFrames = static_cast<std::int64_t> (kRate * 60.0 / 128.0);
    const auto snapped = openmixer::snapToNearest (grid.get(), 10 * beatFrames + 300, 1.0);
    EXPECT_NEAR (static_cast<double> (snapped), static_cast<double> (10 * beatFrames), kRate * 0.01);
}

TEST (BeatGridModel, ConstantGridMapsBeatsAndFrames)
{
    const auto grid = BeatGridModel::constant (120.0, kRate, 1000.0);
    EXPECT_TRUE (grid.isValid());
    EXPECT_FALSE (grid.isManual());
    EXPECT_DOUBLE_EQ (grid.beatAtFrame (1000.0 + kRate), 2.0);
    EXPECT_DOUBLE_EQ (grid.frameAtBeat (-1.0), 1000.0 - kRate * 0.5);

    const openmixer::FixedTempoGrid fixed (120.0, kRate, 1000);
    const AnalysedBeatGrid adapted (grid);
    for (std::int64_t f : { -5000LL, 0LL, 12345LL, 999999LL })
        EXPECT_NEAR (adapted.beatAtFrame (f), fixed.beatAtFrame (f), 1.0e-9);
    EXPECT_EQ (adapted.frameAtBeat (17.25), fixed.frameAtBeat (17.25));
    EXPECT_EQ (makeEngineGrid (BeatGridModel {}), nullptr);
}

TEST (BeatGridModel, ManualCorrections)
{
    auto grid = BeatGridModel::constant (120.0, kRate, 0.0);
    grid.shift (441.0);
    EXPECT_TRUE (grid.isManual());
    EXPECT_DOUBLE_EQ (grid.frameAtBeat (0.0), 441.0);

    grid.setDownbeatNear (441.0 + 3.1 * kRate * 0.5);   // beat 3 becomes a downbeat
    EXPECT_DOUBLE_EQ (std::fmod (grid.beatAtFrame (441.0 + 3.0 * kRate * 0.5), 4.0), 0.0);

    grid.setBpm (125.0);
    EXPECT_DOUBLE_EQ (grid.getBpm(), 125.0);
    grid.setBpm (-3.0);
    EXPECT_DOUBLE_EQ (grid.getBpm(), 125.0);
}

TEST (BeatGridModel, TempoChangeKeepsBeatsContinuous)
{
    auto grid = BeatGridModel::constant (120.0, kRate, 0.0);
    grid.insertTempoChange (10.0 * kRate, 60.0);   // beat 20 at 10 s, then one beat per second
    EXPECT_DOUBLE_EQ (grid.beatAtFrame (10.0 * kRate), 20.0);
    EXPECT_DOUBLE_EQ (grid.beatAtFrame (12.0 * kRate), 22.0);
    EXPECT_DOUBLE_EQ (grid.frameAtBeat (22.0), 12.0 * kRate);
    EXPECT_DOUBLE_EQ (grid.frameAtBeat (10.0), 5.0 * kRate);
    EXPECT_DOUBLE_EQ (grid.bpmAtFrame (11.0 * kRate), 60.0);
    EXPECT_EQ (grid.getMarkers().size(), 2u);
}

TEST (BeatGridModel, JsonRoundTripAndValidation)
{
    auto grid = BeatGridModel::constant (128.0, 48000.0, 512.0, 3);
    grid.insertTempoChange (480000.0, 130.0);
    const auto copy = BeatGridModel::fromVar (juce::JSON::parse (juce::JSON::toString (grid.toVar())));
    ASSERT_TRUE (copy.has_value());
    EXPECT_EQ (copy->getBeatsPerBar(), 3);
    EXPECT_TRUE (copy->isManual());
    EXPECT_DOUBLE_EQ (copy->frameAtBeat (100.0), grid.frameAtBeat (100.0));

    EXPECT_FALSE (BeatGridModel::fromVar (juce::JSON::parse ("{\"version\":1,\"sampleRate\":44100,\"markers\":[]}")).has_value());
    EXPECT_FALSE (BeatGridModel::fromVar (juce::JSON::parse ("{\"version\":1,\"sampleRate\":44100,\"markers\":[{\"frame\":0,\"beat\":0,\"bpm\":-1}]}")).has_value());
    EXPECT_FALSE (BeatGridModel::fromVar (juce::JSON::parse ("{\"version\":9}")).has_value());
}
