// OpenMixer WaveformBuilderTest - multi-resolution levels, peaks, band split, serialisation
#include <JuceHeader.h>
#include "analysis/WaveformBuilder.h"
#include "AnalysisTestSignals.h"

#include <gtest/gtest.h>

using namespace openmixer::analysis;

namespace
{
constexpr double kRate = 44100.0;

WaveformData build (const AudioClip& clip)
{
    NullAnalysisContext context;
    return WaveformBuilder().build (clip, context).value_or (WaveformData {});
}

AudioClip tone (double hz, float gain, double seconds = 5.0)
{
    auto clip = test::silence (seconds, kRate);
    test::addTone (clip, hz, gain);
    return clip;
}

// Mean band levels over the middle of the clip (skips filter settling).
WaveformPoint meanBands (const WaveformData& data)
{
    const auto& points = data.getLevels().front().points;
    WaveformPoint sum;
    const auto from = points.size() / 4, to = points.size() * 3 / 4;
    for (auto i = from; i < to; ++i)
    {
        sum.low += points[i].low; sum.mid += points[i].mid; sum.high += points[i].high;
    }
    const auto n = static_cast<float> (to - from);
    return { 0.0f, 0.0f, sum.low / n, sum.mid / n, sum.high / n };
}
} // namespace

TEST (WaveformBuilder, BuildsLevelsDownToOverview)
{
    const auto data = build (test::silence (60.0, kRate));
    ASSERT_TRUE (data.isValid());
    const auto& levels = data.getLevels();
    ASSERT_EQ (levels.size(), 3u);
    EXPECT_EQ (levels[0].samplesPerPoint, 128);
    EXPECT_EQ (levels[0].points.size(), 20672u);
    EXPECT_EQ (levels[1].samplesPerPoint, 512);
    EXPECT_EQ (levels[2].points.size(), 1292u);
    EXPECT_EQ (&data.getOverview(), &levels[2]);

    EXPECT_EQ (data.levelFor (100.0).samplesPerPoint, 128);
    EXPECT_EQ (data.levelFor (600.0).samplesPerPoint, 512);
    EXPECT_EQ (data.levelFor (1.0e7).samplesPerPoint, 2048);
    EXPECT_EQ (data.getNumSamples(), static_cast<juce::int64> (60.0 * kRate));
}

TEST (WaveformBuilder, PeaksTrackAmplitudeAndTime)
{
    auto clip = test::silence (20.0, kRate);
    test::addTone (clip, 440.0, 0.5f, 10.0, 20.0);
    const auto data = build (clip);

    for (const auto& level : data.getLevels())
    {
        const auto atSeconds = [&] (double s) { return level.points[static_cast<size_t> (s * kRate / level.samplesPerPoint)]; };
        EXPECT_NEAR (atSeconds (5.0).max, 0.0f, 1.0e-6f);
        EXPECT_NEAR (atSeconds (15.0).max, 0.5f, 0.01f);
        EXPECT_NEAR (atSeconds (15.0).min, -0.5f, 0.01f);
    }
}

TEST (WaveformBuilder, SplitsBands)
{
    const auto bass = meanBands (build (tone (60.0, 0.5f)));
    EXPECT_GT (bass.low, 0.3f);
    EXPECT_LT (bass.mid, bass.low * 0.2f);
    EXPECT_LT (bass.high, bass.low * 0.01f);

    const auto mid = meanBands (build (tone (1000.0, 0.5f)));
    EXPECT_GT (mid.mid, 0.3f);
    EXPECT_LT (mid.low, mid.mid * 0.1f);
    EXPECT_LT (mid.high, mid.mid * 0.1f);

    const auto treble = meanBands (build (tone (10000.0, 0.5f)));
    EXPECT_GT (treble.high, 0.3f);
    EXPECT_LT (treble.low, treble.high * 0.01f);
}

TEST (WaveformBuilder, JsonRoundTripKeepsShape)
{
    auto clip = tone (1000.0, 0.7f, 3.0);
    const auto data = build (clip);
    const auto copy = WaveformData::fromJson (data.toJson());
    ASSERT_TRUE (copy.has_value());
    ASSERT_EQ (copy->getLevels().size(), data.getLevels().size());
    EXPECT_EQ (copy->getSampleRate(), kRate);

    const auto& a = data.getLevels()[0].points;
    const auto& b = copy->getLevels()[0].points;
    ASSERT_EQ (a.size(), b.size());
    for (size_t i = 0; i < a.size(); i += 97)
    {
        EXPECT_NEAR (a[i].max, b[i].max, 1.0f / 127.0f);
        EXPECT_NEAR (a[i].min, b[i].min, 1.0f / 127.0f);
        EXPECT_NEAR (a[i].mid, b[i].mid, 0.01f);
    }
    EXPECT_FALSE (WaveformData::fromJson ("{\"version\":1,\"points\":\"%%%\"}").has_value());
    EXPECT_FALSE (WaveformData::fromJson ("{}").has_value());
}

TEST (WaveformBuilder, EmptyClipAndAnalyzer)
{
    const auto empty = build (AudioClip {});
    EXPECT_FALSE (empty.isValid());
    EXPECT_EQ (empty.levelFor (100.0).points.size(), 0u);
    const auto parsed = WaveformData::fromJson (empty.toJson());
    ASSERT_TRUE (parsed.has_value());
    EXPECT_FALSE (parsed->isValid());

    WaveformAnalyzer analyzer;
    NullAnalysisContext context;
    const auto payload = analyzer.analyze (tone (200.0, 0.3f, 2.0), context);
    ASSERT_TRUE (payload.has_value());
    const auto data = WaveformData::fromJson (*payload);
    ASSERT_TRUE (data.has_value());
    EXPECT_NEAR (data->getOverview().points.front().max, 0.3f, 0.02f);
}
