// OpenMixer MixerTest - EQ, filter, faders, crossfader, cue bus, meters, parameters and inserts
#include <JuceHeader.h>
#include "engine/Mixer.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <cmath>
#include <vector>

using namespace openmixer;

namespace
{
constexpr double sr = 48000.0;
constexpr int N = 16384;

struct Rig
{
    Mixer mixer { 2 };
    std::vector<float> in[2][2];   // [channel][L/R]
    std::vector<float> mL, mR, cL, cR;

    Rig() : mL (N), mR (N), cL (N), cR (N)
    {
        mixer.prepare (sr, 512);
        for (auto& c : in) for (auto& v : c) v.assign (N, 0.0f);
    }

    void setSine (int ch, double hz, float amp = 0.5f)
    {
        for (int i = 0; i < N; ++i)
        {
            const float s = amp * static_cast<float> (std::sin (2.0 * juce::MathConstants<double>::pi * hz * i / sr));
            in[ch][0][static_cast<size_t> (i)] = s;
            in[ch][1][static_cast<size_t> (i)] = s;
        }
    }

    void run()
    {
        Mixer::ChannelInput ci[2] = { { in[0][0].data(), in[0][1].data() },
                                      { in[1][0].data(), in[1][1].data() } };
        for (int off = 0; off < N; off += 512)   // block-wise like a device
        {
            Mixer::ChannelInput blk[2];
            for (int c = 0; c < 2; ++c) { blk[c].left = ci[c].left + off; blk[c].right = ci[c].right + off; }
            mixer.process (blk, mL.data() + off, mR.data() + off, cL.data() + off, cR.data() + off, 512);
        }
    }

    static double rms (const std::vector<float>& v)
    {
        double s = 0;
        int cnt = 0;
        for (size_t i = N / 2; i < v.size(); ++i) { s += v[i] * v[i]; ++cnt; }
        return std::sqrt (s / cnt);
    }
};

double dbOf (double ratio) { return 20.0 * std::log10 (std::max (ratio, 1e-12)); }

/** Gain in dB through master for a sine at hz with the given setup. */
template <typename Setup>
double gainDb (double hz, Setup setup)
{
    Rig r;
    r.setSine (0, hz);
    setup (r.mixer);
    r.run();
    return dbOf (Rig::rms (r.mL) / (0.5 / std::sqrt (2.0)));
}
}

TEST (MixerTest, FlatSettingsAreUnityGain)
{
    for (double hz : { 60.0, 1000.0, 10000.0 })
        EXPECT_NEAR (gainDb (hz, [] (Mixer&) {}), 0.0, 0.1) << "hz=" << hz;
}

TEST (MixerTest, ThreeBandEqCutsAndBoostsTheIntendedBand)
{
    EXPECT_LT (gainDb (60.0,    [] (Mixer& m) { m.setEq (0, 0, -48.0f); }), -30.0) << "low kill";
    EXPECT_NEAR (gainDb (10000.0, [] (Mixer& m) { m.setEq (0, 0, -48.0f); }), 0.0, 0.5);
    EXPECT_LT (gainDb (1000.0,  [] (Mixer& m) { m.setEq (0, 1, -48.0f); }), -30.0) << "mid kill";
    EXPECT_NEAR (gainDb (60.0, [] (Mixer& m) { m.setEq (0, 1, -48.0f); }), 0.0, 5.0);
    EXPECT_LT (gainDb (12000.0, [] (Mixer& m) { m.setEq (0, 2, -48.0f); }), -30.0) << "high kill";
    EXPECT_NEAR (gainDb (100.0, [] (Mixer& m) { m.setEq (0, 2, -48.0f); }), 0.0, 0.5);
    EXPECT_NEAR (gainDb (60.0, [] (Mixer& m) { m.setEq (0, 0, 6.0f); }), 6.0, 1.0);
    EXPECT_NEAR (gainDb (1000.0, [] (Mixer& m) { m.setEq (0, 1, 6.0f); }), 6.0, 0.3);
    EXPECT_NEAR (gainDb (12000.0, [] (Mixer& m) { m.setEq (0, 2, 6.0f); }), 6.0, 1.0);
}

TEST (MixerTest, SweepFilterLowPassHighPassAndBypass)
{
    EXPECT_LT (gainDb (10000.0, [] (Mixer& m) { m.setFilter (0, -0.8f); }), -20.0) << "LP removes highs";
    EXPECT_NEAR (gainDb (40.0, [] (Mixer& m) { m.setFilter (0, -0.8f); }), 0.0, 1.0);
    EXPECT_LT (gainDb (100.0, [] (Mixer& m) { m.setFilter (0, 0.8f); }), -20.0) << "HP removes lows";
    EXPECT_NEAR (gainDb (10000.0, [] (Mixer& m) { m.setFilter (0, 0.8f); }), 0.0, 1.0);
    EXPECT_NEAR (gainDb (100.0, [] (Mixer& m) { m.setFilter (0, 0.01f); }), 0.0, 0.01);
}

TEST (MixerTest, ChannelFaderAndMasterGainScaleOutput)
{
    EXPECT_NEAR (gainDb (1000.0, [] (Mixer& m) { m.setFader (0, 0.5f); }), -6.02, 0.1);
    EXPECT_LT (gainDb (1000.0, [] (Mixer& m) { m.setFader (0, 0.0f); }), -100.0);
    EXPECT_NEAR (gainDb (1000.0, [] (Mixer& m) { m.setMasterGain (0.5f); }), -6.02, 0.1);
    EXPECT_NEAR (gainDb (1000.0, [] (Mixer& m) { m.setTrim (0, 2.0f); }), 6.02, 0.1);
}

TEST (MixerTest, ParametersAreClamped)
{
    Mixer m (2);
    m.setFader (0, 5.0f);
    EXPECT_EQ (m.getFader (0), 1.0f);
    m.setEq (0, 1, -999.0f);
    EXPECT_EQ (m.getEq (0, 1), Mixer::kMinEqDb);
    m.setFilter (1, -3.0f);
    EXPECT_EQ (m.getFilter (1), -1.0f);
    m.setFader (99, 0.25f);
    EXPECT_EQ (m.getFader (1), 0.25f);   // index clamps
}

TEST (MixerTest, CrossfaderRoutesAssignedChannels)
{
    for (auto curve : { CrossfaderCurve::Linear, CrossfaderCurve::ConstantPower })
    {
        SCOPED_TRACE (curve == CrossfaderCurve::Linear ? "Linear" : "ConstantPower");
        Rig r;
        r.setSine (0, 1000.0);
        r.setSine (1, 3000.0);
        r.mixer.setCrossfaderCurve (curve);
        r.mixer.setCrossfaderAssign (0, CrossfaderAssign::Left);
        r.mixer.setCrossfaderAssign (1, CrossfaderAssign::Right);

        r.mixer.setCrossfader (0.0f);
        r.run();
        EXPECT_NEAR (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);   // only ch0

        r.mixer.setCrossfader (1.0f);
        r.run();
        EXPECT_NEAR (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);   // only ch1

        r.mixer.setCrossfader (0.5f);
        r.run();
        // Two uncorrelated equal sines: powers sum.
        const double g = curve == CrossfaderCurve::Linear ? 0.5 : std::sqrt (0.5);
        const double expected = std::sqrt (2.0 * g * g) * 0.5 / std::sqrt (2.0);
        EXPECT_NEAR (Rig::rms (r.mL), expected, 0.01);
    }
}

TEST (MixerTest, ThruChannelsIgnoreTheCrossfader)
{
    Rig r;
    r.setSine (0, 1000.0);
    r.mixer.setCrossfader (1.0f);
    r.run();
    EXPECT_NEAR (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);
}

TEST (MixerTest, PflCueBusIsPreFaderAndIndependentOfCrossfader)
{
    Rig r;
    r.setSine (0, 1000.0);
    r.setSine (1, 3000.0);
    r.mixer.setCrossfaderAssign (0, CrossfaderAssign::Left);
    r.mixer.setCrossfader (1.0f);   // ch0 fully cut from master
    r.mixer.setFader (0, 0.0f);
    r.mixer.setPfl (0, true);
    r.run();
    const double unity = 0.5 / std::sqrt (2.0);
    EXPECT_GT (Rig::rms (r.cL), unity * 0.98) << "cue carries ch0 at full level";
    EXPECT_LT (Rig::rms (r.cL), unity * 1.02) << "cue carries ch0 at full level";
    EXPECT_GT (Rig::rms (r.mL), unity * 0.98) << "master carries only ch1";
    EXPECT_LT (Rig::rms (r.mL), unity * 1.02) << "master carries only ch1";

    r.mixer.setPfl (0, false);
    r.run();
    EXPECT_LT (Rig::rms (r.cL), 1e-6) << "cue is silent without PFL";
}

TEST (MixerTest, CueMixBlendsMasterIntoHeadphonesAndCueLevelScales)
{
    Rig r;
    r.setSine (0, 1000.0);
    r.setSine (1, 3000.0);
    r.mixer.setPfl (0, true);
    r.mixer.setFader (0, 0.0f);   // ch0 only in cue
    r.mixer.setCueMix (1.0f);
    r.run();
    const double masterRms = Rig::rms (r.mL);
    EXPECT_NEAR (Rig::rms (r.cL), masterRms, 1e-4);
    r.mixer.setCueMix (0.5f);
    r.mixer.setCueLevel (0.5f);
    r.run();
    EXPECT_GT (Rig::rms (r.cL), 0.0);
    EXPECT_LT (Rig::rms (r.cL), masterRms) << "scaled below master";
}

TEST (MixerTest, MetersTrackLevelAndDecay)
{
    Rig r;
    r.setSine (0, 1000.0, 0.5f);
    r.run();
    EXPECT_GT (r.mixer.getChannelPeak (0), 0.1f);
    EXPECT_GT (r.mixer.getMasterPeakLeft(), 0.1f);
    EXPECT_NEAR (r.mixer.getChannelPeak (1), 0.0f, 1e-6f);
    r.setSine (0, 1000.0, 0.0f);
    for (int i = 0; i < 6; ++i) r.run();
    EXPECT_LT (r.mixer.getChannelPeak (0), 0.01f) << "peak decays after silence";
}

TEST (MixerTest, SetParameterDrivesTheSameControlsByStringId)
{
    Mixer m (2);
    EXPECT_TRUE (m.setParameter ("ch1.fader", 0.25));
    EXPECT_EQ (m.getFader (1), 0.25f);
    EXPECT_TRUE (m.setParameter ("ch0.eq.high", -6.0));
    EXPECT_EQ (m.getEq (0, 2), -6.0f);
    EXPECT_TRUE (m.setParameter ("ch0.pfl", 1.0));
    EXPECT_TRUE (m.getPfl (0));
    EXPECT_TRUE (m.setParameter ("ch0.filter", -0.5));
    EXPECT_EQ (m.getFilter (0), -0.5f);
    EXPECT_TRUE (m.setParameter ("crossfader", 0.9));
    EXPECT_TRUE (m.setParameter ("master", 1.0));
    EXPECT_TRUE (m.setParameter ("cue.level", 1.0));
    EXPECT_TRUE (m.setParameter ("cue.mix", 0.5));
    EXPECT_FALSE (m.setParameter ("ch2.fader", 1.0)) << "channel out of range";
    EXPECT_FALSE (m.setParameter ("ch0.nonsense", 1.0));
    EXPECT_FALSE (m.setParameter ("ch0.eq.sub", 1.0));
    EXPECT_FALSE (m.setParameter ("bogus", 1.0));
}

namespace
{
struct Gain : ProcessStage
{
    float g;
    explicit Gain (float v) : g (v) {}
    void process (float* l, float* r, int n) noexcept override
    {
        for (int i = 0; i < n; ++i) { l[i] *= g; r[i] *= g; }
    }
};

struct Stale : ProcessStage
{
    int interfaceVersion() const noexcept override { return 999; }
    void process (float*, float*, int) noexcept override {}
};
}

TEST (MixerTest, ChannelAndMasterInsertsCanBeAddedReplacedAndRemovedAtRuntime)
{
    Rig r;
    r.setSine (0, 1000.0);
    Gain half (0.5f), quarter (0.25f), masterDouble (2.0f);
    Stale stale;
    const double unity = 0.5 / std::sqrt (2.0);

    EXPECT_TRUE (r.mixer.setChannelInsert (0, 0, &half));
    r.run();
    EXPECT_NEAR (Rig::rms (r.mL), unity * 0.5, 0.005);

    EXPECT_TRUE (r.mixer.setChannelInsert (0, 0, &quarter)) << "replace while running";
    r.run();
    EXPECT_NEAR (Rig::rms (r.mL), unity * 0.25, 0.005);

    EXPECT_TRUE (r.mixer.setMasterInsert (0, &masterDouble));
    r.run();
    EXPECT_NEAR (Rig::rms (r.mL), unity * 0.5, 0.005);

    EXPECT_TRUE (r.mixer.setChannelInsert (0, 0, nullptr));
    EXPECT_TRUE (r.mixer.setMasterInsert (0, nullptr));
    r.run();
    EXPECT_NEAR (Rig::rms (r.mL), unity, 0.005);

    EXPECT_FALSE (r.mixer.setChannelInsert (0, 0, &stale)) << "incompatible interface version";
    EXPECT_FALSE (r.mixer.setChannelInsert (0, 99, &half)) << "bad slot";
}

TEST (MixerTest, NullInputsProduceSilenceAndOversizeBlocksAreChunked)
{
    Rig r;
    Mixer::ChannelInput none[2];
    r.mixer.process (none, r.mL.data(), r.mR.data(), r.cL.data(), r.cR.data(), 512);
    for (size_t i = 0; i < 512; ++i)
        EXPECT_EQ (r.mL[i], 0.0f);

    r.setSine (0, 1000.0);
    Mixer::ChannelInput ci[2] = { { r.in[0][0].data(), r.in[0][1].data() }, {} };
    r.mixer.process (ci, r.mL.data(), r.mR.data(), r.cL.data(), r.cR.data(), N);   // N > prepared 512
    EXPECT_NEAR (Rig::rms (r.mL), 0.5 / std::sqrt (2.0), 0.005);
}
