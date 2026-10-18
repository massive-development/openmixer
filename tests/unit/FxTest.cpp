// OpenMixer FxTest - FX rack: beat-synced timing, wet/dry, tails, bounds, ids, presets
#include <JuceHeader.h>
#include "engine/Mixer.h"
#include "engine/fx/FxPresets.h"
#include "engine/fx/DelayLine.h"
#include "engine/fx/FxRack.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <cmath>
#include <set>
#include <vector>

using namespace openmixer;
using namespace openmixer::fx;

namespace
{
constexpr double sr = 48000.0;

struct Signal
{
    std::vector<float> l, r;
    explicit Signal (size_t n) : l (n, 0.0f), r (n, 0.0f) {}
    size_t size() const { return l.size(); }
};

Signal sine (size_t n, double hz, float amp = 0.5f)
{
    Signal s (n);
    for (size_t i = 0; i < n; ++i)
        s.l[i] = s.r[i] = amp * static_cast<float> (std::sin (2.0 * juce::MathConstants<double>::pi * hz * static_cast<double> (i) / sr));
    return s;
}

Signal noise (size_t n, float amp = 1.0f)
{
    juce::Random rng (1234);
    Signal s (n);
    for (size_t i = 0; i < n; ++i)
    {
        s.l[i] = amp * (rng.nextFloat() * 2.0f - 1.0f);
        s.r[i] = amp * (rng.nextFloat() * 2.0f - 1.0f);
    }
    return s;
}

void run (ProcessStage& stage, Signal& s, size_t from = 0, size_t to = 0, int block = 512)
{
    to = to == 0 ? s.size() : to;
    for (size_t off = from; off < to; off += static_cast<size_t> (block))
    {
        const auto n = static_cast<int> (std::min (static_cast<size_t> (block), to - off));
        stage.process (s.l.data() + off, s.r.data() + off, n);
    }
}

std::unique_ptr<FxUnit> makeUnit (const std::string& name, bool enabled = true, float mix = 1.0f)
{
    auto unit = std::make_unique<FxUnit> (FxFactory::withBuiltIns().create (name));
    unit->setEnabled (enabled);
    unit->setMix (mix);
    return unit;
}

size_t argmaxAbs (const std::vector<float>& v)
{
    return static_cast<size_t> (std::distance (v.begin(), std::max_element (v.begin(), v.end(),
        [] (float a, float b) { return std::abs (a) < std::abs (b); })));
}

bool allFinite (const Signal& s)
{
    auto finite = [] (float x) { return std::isfinite (x); };
    return std::all_of (s.l.begin(), s.l.end(), finite) && std::all_of (s.r.begin(), s.r.end(), finite);
}

float peak (const Signal& s, size_t from = 0, size_t to = 0)
{
    to = to == 0 ? s.size() : to;
    float p = 0.0f;
    for (size_t i = from; i < to; ++i)
        p = std::max ({ p, std::abs (s.l[i]), std::abs (s.r[i]) });
    return p;
}

// Echo with no feedback: the only non-zero output is the first repeat of an impulse.
size_t firstEchoPosition (double bpm, float beats, int block)
{
    auto unit = makeUnit ("echo");
    unit->setTempo (bpm);
    unit->setParameter ("time", beats);
    unit->setParameter ("feedback", 0.0);
    unit->prepare (sr, block);
    Signal s (60000);
    s.l[0] = s.r[0] = 1.0f;
    run (*unit, s, 0, 0, block);
    return argmaxAbs (s.l);
}
} // namespace

TEST (FxFactory, RegistersEightUniqueBuiltIns)
{
    const auto names = FxFactory::builtInNames();
    ASSERT_EQ (names.size(), 8u);
    EXPECT_EQ (std::set<std::string> (names.begin(), names.end()).size(), 8u);
    const auto factory = FxFactory::withBuiltIns();
    for (const auto& n : names)
    {
        auto fx = factory.create (n);
        ASSERT_NE (fx, nullptr) << n;
        EXPECT_EQ (fx->getName(), n);
    }
    EXPECT_EQ (factory.create ("nope"), nullptr);
}

TEST (FxFactory, PluginsCanAddButNotOverride)
{
    auto factory = FxFactory::withBuiltIns();
    EXPECT_FALSE (factory.registerEffect ("echo", [] { return FxFactory::withBuiltIns().create ("gate"); }));
    EXPECT_TRUE (factory.registerEffect ("mygate", [] { return FxFactory::withBuiltIns().create ("gate"); }));
    EXPECT_FALSE (factory.registerEffect ("", [] { return FxFactory::withBuiltIns().create ("gate"); }));
    EXPECT_NE (factory.create ("mygate"), nullptr);
    FxRack rack ({ "mygate", "missing", "echo" }, factory);
    EXPECT_EQ (rack.getNumUnits(), 2);
    EXPECT_EQ (rack.findUnit ("echo"), 1);
}

TEST (FxEcho, RepeatLandsOnTheBeatGridAtAnyTempo)
{
    EXPECT_EQ (firstEchoPosition (120.0, 1.0f, 512), 24000u);
    EXPECT_EQ (firstEchoPosition (90.0, 1.0f, 512), 32000u);
    EXPECT_EQ (firstEchoPosition (120.0, 0.5f, 512), 12000u);
    EXPECT_EQ (firstEchoPosition (128.0, 0.75f, 37), 16875u);   // odd block size crosses chunks
}

TEST (FxEcho, FeedbackRepeatsDecay)
{
    auto unit = makeUnit ("echo");
    unit->setTempo (120.0);
    unit->setParameter ("time", 0.25);   // 6000 samples
    unit->setParameter ("feedback", 0.5);
    unit->prepare (sr, 512);
    Signal s (48000);
    s.l[0] = s.r[0] = 1.0f;
    run (*unit, s);
    const float first = peak (s, 5000, 7000), second = peak (s, 11000, 13000), third = peak (s, 17000, 19000);
    EXPECT_GT (first, 0.9f);
    EXPECT_LT (second, first);
    EXPECT_LT (third, second);
    EXPECT_GT (third, 0.01f);
}

TEST (FxUnit, MixZeroAndDisabledAreBitExactDry)
{
    for (const auto& name : FxFactory::builtInNames())
    {
        for (const bool enabled : { true, false })
        {
            auto unit = makeUnit (name, enabled, enabled ? 0.0f : 1.0f);
            unit->prepare (sr, 512);
            const auto ref = sine (24000, 220.0);
            auto s = ref;
            run (*unit, s);
            EXPECT_EQ (s.l, ref.l) << name << " enabled=" << enabled;
            EXPECT_EQ (s.r, ref.r) << name << " enabled=" << enabled;
        }
    }
}

TEST (FxUnit, EveryEffectStaysFiniteAndBoundedAtExtremes)
{
    for (const auto& name : FxFactory::builtInNames())
    {
        for (const bool atMax : { true, false })
        {
            auto unit = makeUnit (name);
            unit->setTempo (atMax ? 250.0 : 50.0);
            auto& fx = unit->getEffect();
            for (int p = 0; p < fx.getNumParams(); ++p)
                fx.setParam (p, atMax ? fx.getParamSpec (p).maxValue : fx.getParamSpec (p).minValue);
            unit->prepare (sr, 512);
            auto s = noise (96000);
            run (*unit, s);
            EXPECT_TRUE (allFinite (s)) << name;
            EXPECT_LT (peak (s), 4.0f) << name << " atMax=" << atMax;
        }
    }
}

TEST (FxUnit, EveryEffectAltersTheSignalWhenFullyWet)
{
    for (const auto& name : FxFactory::builtInNames())
    {
        auto unit = makeUnit (name);
        unit->prepare (sr, 512);
        const auto ref = noise (48000, 0.5f);
        auto s = ref;
        run (*unit, s);
        double diff = 0.0;
        for (size_t i = 0; i < s.size(); ++i)
            diff += std::abs (s.l[i] - ref.l[i]);
        EXPECT_GT (diff / static_cast<double> (s.size()), 0.01) << name;
    }
}

TEST (FxUnit, RejectsNonFiniteAndClampsParameters)
{
    auto unit = makeUnit ("echo");
    EXPECT_FALSE (unit->setParameter ("time", std::nan ("")));
    EXPECT_TRUE (unit->setParameter ("feedback", 5.0));
    auto& fx = unit->getEffect();
    EXPECT_FLOAT_EQ (fx.getParamTarget (fx.findParam ("feedback")), 0.95f);
    unit->setMix (-1.0f);
    EXPECT_FLOAT_EQ (unit->getMix(), 0.0f);
    unit->setTempo (1000.0);
    EXPECT_DOUBLE_EQ (unit->getTempo(), kMaxBpm);
}

TEST (FxUnit, TailRingsOutAfterDisableThenGoesIdle)
{
    for (const std::string name : { "echo", "reverb" })
    {
        auto unit = makeUnit (name, true, 0.5f);
        unit->setTempo (120.0);
        unit->setParameter ("time", 0.25);
        unit->prepare (sr, 512);
        Signal s (static_cast<size_t> (sr * 20));
        auto burst = noise (4800, 0.5f);
        std::copy (burst.l.begin(), burst.l.end(), s.l.begin());
        std::copy (burst.r.begin(), burst.r.end(), s.r.begin());
        run (*unit, s, 0, 4800);
        unit->setEnabled (false);
        run (*unit, s, 4800);
        EXPECT_GT (peak (s, 9600, 20000), 1.0e-3f) << name << ": tail cut on disable";
        EXPECT_FALSE (unit->isActive()) << name << ": tail never finished";
        EXPECT_EQ (peak (s, s.size() - 4800), 0.0f) << name;
    }
}

TEST (FxUnit, NonTailEffectFadesOutAndStopsWhenDisabled)
{
    auto unit = makeUnit ("gate");
    unit->prepare (sr, 512);
    auto s = sine (48000, 330.0);
    const auto ref = s;
    run (*unit, s, 0, 9600);
    unit->setEnabled (false);
    run (*unit, s, 9600);
    EXPECT_FALSE (unit->isActive());
    EXPECT_TRUE (std::equal (s.l.begin() + 24000, s.l.end(), ref.l.begin() + 24000));
}

TEST (DelayLine, ClearsInSlicesNotAllAtOnce)
{
    DelayLine line;
    line.allocate (5000);
    for (int i = 0; i < 5000; ++i)
        line.push (1.0f, 1.0f);
    line.beginClear();
    int slices = 0;
    while (! line.clearSome (512))
        ++slices;
    EXPECT_GE (slices, 9) << "a 512-frame slice must not wipe the whole line";
    for (int d = 1; d < 5000; d += 97)
        EXPECT_EQ (line.read (0, static_cast<float> (d)), 0.0f);
    EXPECT_TRUE (line.clearSome (512)) << "nothing left once cleared";
}

TEST (FxUnit, IdleClearIsAmortisedAndEngagingEarlyStillStartsClean)
{
    for (const std::string name : { "flanger", "echo" })
    {
        auto unit = makeUnit (name, true, 1.0f);
        unit->setParameter ("feedback", 0.3);
        unit->prepare (sr, 512);
        auto loud = sine (static_cast<size_t> (sr), 330.0, 0.8f);
        run (*unit, loud);
        unit->setEnabled (false);
        auto quiet = Signal (static_cast<size_t> (sr * 10));
        run (*unit, quiet);
        ASSERT_FALSE (unit->isActive()) << name;
        unit->setEnabled (true);   // re-engage right after idle; wipe may still be pending
        Signal silence (4096);
        run (*unit, silence);
        EXPECT_LT (peak (silence), 1.0e-5f) << name << ": stale audio leaked after re-engage";
    }
}

TEST (FxUnit, MixChangesAreSmoothed)
{
    auto unit = makeUnit ("echo", true, 0.0f);
    unit->setTempo (120.0);
    unit->setParameter ("time", 0.51);   // 12240 samples: half a 100 Hz cycle past a whole number
    unit->setParameter ("feedback", 0.0);
    unit->prepare (sr, 512);
    auto s = sine (48000, 100.0);
    run (*unit, s, 0, 24576);
    unit->setMix (1.0f);   // dry and wet are 180 degrees apart here: a hard switch would jump
    run (*unit, s, 24576);
    float maxStep = 0.0f;
    for (size_t i = 24000; i < 30000; ++i)
        maxStep = std::max (maxStep, std::abs (s.l[i] - s.l[i - 1]));
    EXPECT_LT (maxStep, 0.02f);
}

TEST (FxUnit, NoSubnormalsInDecayingReverb)
{
    auto unit = makeUnit ("reverb");
    unit->prepare (sr, 512);
    Signal s (static_cast<size_t> (sr * 10));
    s.l[0] = s.r[0] = 1.0f;
    run (*unit, s);
    const auto subnormal = [] (float x) { return std::fpclassify (x) == FP_SUBNORMAL; };
    EXPECT_FALSE (std::any_of (s.l.begin(), s.l.end(), subnormal));
    EXPECT_FALSE (std::any_of (s.r.begin(), s.r.end(), subnormal));
}

TEST (FxRoll, RepeatsOneBeatDivision)
{
    auto unit = makeUnit ("roll");
    unit->setTempo (120.0);
    unit->setParameter ("time", 0.25);   // 6000 samples
    unit->prepare (sr, 512);
    auto s = sine (30000, 173.0);
    const auto ref = s;
    run (*unit, s);
    EXPECT_FLOAT_EQ (s.l[12000 + 3000], ref.l[3000]);
    EXPECT_FLOAT_EQ (s.l[24000 + 4500], ref.l[4500]);
    EXPECT_FLOAT_EQ (s.l[3000], ref.l[3000]);
}

TEST (FxGate, ChopsOnBeatDivisions)
{
    auto unit = makeUnit ("gate");
    unit->setTempo (120.0);
    unit->setParameter ("time", 0.5);   // 12000-sample period, open for the first half
    unit->setParameter ("duty", 0.5);
    unit->prepare (sr, 512);
    Signal s (36000);
    std::fill (s.l.begin(), s.l.end(), 0.5f);
    std::fill (s.r.begin(), s.r.end(), 0.5f);
    run (*unit, s);
    EXPECT_NEAR (s.l[3000], 0.5f, 1.0e-3f);
    EXPECT_NEAR (s.l[9000], 0.0f, 1.0e-3f);
    EXPECT_NEAR (s.l[15000], 0.5f, 1.0e-3f);
}

TEST (FxTapeStop, SlowsToSilenceInTheSyncedTime)
{
    auto unit = makeUnit ("tapestop");
    unit->setTempo (120.0);
    unit->setParameter ("time", 1.0);   // stops within 24000 samples
    unit->prepare (sr, 512);
    auto s = sine (48000, 440.0);
    run (*unit, s);
    EXPECT_GT (peak (s, 0, 4800), 0.4f);
    EXPECT_EQ (peak (s, 24100), 0.0f);
}

TEST (FxRack, ParameterIdsRouteToUnits)
{
    FxRack rack;
    ASSERT_EQ (rack.getNumUnits(), 8);
    EXPECT_TRUE (rack.setParameter ("fx0.mix", 0.3));
    EXPECT_FLOAT_EQ (rack.getUnit (0)->getMix(), 0.3f);
    EXPECT_TRUE (rack.setParameter ("fx0.enable", 1.0));
    EXPECT_TRUE (rack.getUnit (0)->isEnabled());
    EXPECT_TRUE (rack.setParameter ("fx0.time", 2.0));
    const auto& echo = rack.getUnit (0)->getEffect();
    EXPECT_FLOAT_EQ (echo.getParamTarget (echo.findParam ("time")), 2.0f);
    EXPECT_TRUE (rack.setParameter ("fx7.time", 4.0));
    for (const auto* bad : { "fx8.mix", "fx0.bogus", "fxa.mix", "fx.mix", "fx0", "mix", "" })
        EXPECT_FALSE (rack.setParameter (bad, 0.5)) << bad;
}

TEST (FxRack, TempoFromIdAndBeatGrid)
{
    FxRack rack;
    EXPECT_TRUE (rack.setParameter ("bpm", 128.0));
    EXPECT_DOUBLE_EQ (rack.getUnit (3)->getTempo(), 128.0);
    rack.setTempoFromGrid (FixedTempoGrid (100.0, sr), 1.08);
    for (int i = 0; i < rack.getNumUnits(); ++i)
        EXPECT_DOUBLE_EQ (rack.getUnit (i)->getTempo(), 108.0);
}

TEST (FxRack, AllUnitsTogetherStayBounded)
{
    FxRack rack;
    for (int i = 0; i < rack.getNumUnits(); ++i)
        rack.getUnit (i)->setEnabled (true);
    rack.prepare (sr, 256);
    auto s = sine (96000, 110.0, 0.9f);
    run (rack, s, 0, 0, 256);
    EXPECT_TRUE (allFinite (s));
    EXPECT_LT (peak (s), 4.0f);
}

TEST (FxRack, InsertsIntoMixerMasterWithoutColouringWhenOff)
{
    constexpr int n = 4096;
    Mixer plain { 1 }, withFx { 1 };
    FxRack rack;
    plain.prepare (sr, 512);
    withFx.prepare (sr, 512);
    ASSERT_TRUE (withFx.setMasterInsert (0, &rack));
    const auto in = sine (n, 440.0);
    std::vector<float> aL (n), aR (n), bL (n), bR (n), cue (n);
    for (int off = 0; off < n; off += 512)
    {
        const auto o = static_cast<size_t> (off);
        Mixer::ChannelInput ch { in.l.data() + o, in.r.data() + o };
        plain.process (&ch, aL.data() + o, aR.data() + o, cue.data() + o, cue.data() + o, 512);
        withFx.process (&ch, bL.data() + o, bR.data() + o, cue.data() + o, cue.data() + o, 512);
    }
    EXPECT_EQ (aL, bL);
    EXPECT_EQ (aR, bR);
    withFx.setMasterInsert (0, nullptr);
}

TEST (FxPresets, OriginalPresetsApplyToMatchingEffects)
{
    const auto& presets = builtInPresets();
    EXPECT_EQ (presets.size(), 8u);
    FxRack rack;
    for (const auto& preset : presets)
    {
        const int idx = rack.findUnit (preset.effect);
        ASSERT_GE (idx, 0) << preset.name;
        EXPECT_TRUE (applyPreset (*rack.getUnit (idx), preset));
        EXPECT_FLOAT_EQ (rack.getUnit (idx)->getMix(), preset.mix);
    }
    EXPECT_FALSE (applyPreset (*rack.getUnit (0), *findPreset ("Half Roll")));
    EXPECT_EQ (findPreset ("does not exist"), nullptr);
}
