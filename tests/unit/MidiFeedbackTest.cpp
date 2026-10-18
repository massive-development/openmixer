// OpenMixer MidiFeedbackTest - output rules, change-only sending, blink on an injected clock, engine state ids
#include <JuceHeader.h>
#include "EngineStateReader.h"
#include "controllers/MidiFeedback.h"

#include <gtest/gtest.h>

#include <map>

using namespace openmixer;

namespace
{
struct FakeState final : StateReader
{
    bool readState (const juce::String& id, double& out) const override
    {
        const auto it = values.find (id);
        if (it == values.end())
            return false;
        out = it->second;
        return true;
    }
    std::map<juce::String, double> values;
};
}

class MidiFeedbackTest : public ::testing::Test
{
protected:
    void load (const char* outputsJson)
    {
        ASSERT_TRUE (mapper.loadFromJson (juce::String (R"({"version":2,"mappings":[],"outputs":)") + outputsJson + "}").wasOk());
    }

    FakeState state;
    MidiMapper mapper;
    double now = 0.0;
    MidiFeedback feedback { state, mapper, [this] { return now; } };
    RecordingOutputSink sink;
};

TEST_F (MidiFeedbackTest, NothingIsSentWithoutASink)
{
    load (R"([{"source":"deck.0.play","type":"note","number":36}])");
    state.values["deck.0.play"] = 1.0;
    EXPECT_EQ (feedback.update(), 0);
}

TEST_F (MidiFeedbackTest, SendsOnlyWhenTheValueChanges)
{
    load (R"([{"source":"deck.0.play","type":"note","channel":2,"number":36,"on":100,"off":0}])");
    feedback.setSink (&sink);
    state.values["deck.0.play"] = 0.0;
    EXPECT_EQ (feedback.update(), 1) << "initial state is sent once";
    EXPECT_EQ (feedback.update(), 0);
    EXPECT_EQ (feedback.update(), 0);
    state.values["deck.0.play"] = 1.0;
    EXPECT_EQ (feedback.update(), 1);
    EXPECT_EQ (feedback.update(), 0);
    ASSERT_EQ (sink.sent.size(), 2u);
    EXPECT_TRUE (sink.sent[0].isNoteOff());
    EXPECT_TRUE (sink.sent[1].isNoteOn());
    EXPECT_EQ (sink.sent[1].getChannel(), 2);
    EXPECT_EQ (sink.sent[1].getNoteNumber(), 36);
    EXPECT_EQ (sink.sent[1].getVelocity(), 100);
}

TEST_F (MidiFeedbackTest, LevelRuleScalesAndSendsCcOnlyWhenTheQuantisedValueMoves)
{
    load (R"([{"source":"mixer.master.level","type":"cc","number":9,"mode":"level"}])");
    feedback.setSink (&sink);
    state.values["mixer.master.level"] = 0.5;
    feedback.update();
    state.values["mixer.master.level"] = 0.5001;   // same 0-127 step
    EXPECT_EQ (feedback.update(), 0);
    state.values["mixer.master.level"] = 1.5;      // clamps at the top
    feedback.update();
    ASSERT_EQ (sink.sent.size(), 2u);
    EXPECT_TRUE (sink.sent[0].isController());
    EXPECT_EQ (sink.sent[0].getControllerValue(), 64);
    EXPECT_EQ (sink.sent[1].getControllerValue(), 127);
}

TEST_F (MidiFeedbackTest, BlinkAlternatesEachHalfPeriodWhileTheStateIsOn)
{
    load (R"([{"source":"deck.0.cue","type":"note","number":37,"blink":"on","blinkMs":100}])");
    feedback.setSink (&sink);
    state.values["deck.0.cue"] = 1.0;
    std::vector<bool> lit;
    for (now = 0.0; now < 450.0; now += 10.0)
        if (feedback.update() > 0)
            lit.push_back (sink.sent.back().isNoteOn());
    EXPECT_EQ (lit, (std::vector<bool> { true, false, true, false, true }));

    state.values["deck.0.cue"] = 0.0;   // off: steady, no more blinking
    now = 500.0;
    feedback.update();
    const auto n = sink.sent.size();
    for (now = 510.0; now < 900.0; now += 10.0)
        feedback.update();
    EXPECT_EQ (sink.sent.size(), n);
    EXPECT_TRUE (sink.sent.back().isNoteOff());
}

TEST_F (MidiFeedbackTest, BlinkWhenOffBlinksOnlyWhileOff)
{
    load (R"([{"source":"deck.0.loaded","type":"note","number":38,"blink":"off","blinkMs":50}])");
    feedback.setSink (&sink);
    state.values["deck.0.loaded"] = 0.0;
    for (now = 0.0; now < 200.0; now += 10.0)
        feedback.update();
    EXPECT_EQ (sink.sent.size(), 4u);
    state.values["deck.0.loaded"] = 1.0;
    now = 200.0;
    feedback.update();
    const auto n = sink.sent.size();
    now = 260.0;
    EXPECT_EQ (feedback.update(), 0);
    EXPECT_EQ (sink.sent.size(), n);
    EXPECT_TRUE (sink.sent.back().isNoteOn());
}

TEST_F (MidiFeedbackTest, UnknownSourcesAreSkippedAndAMapReloadResendsEverything)
{
    load (R"([{"source":"nope.x","type":"note","number":1},{"source":"a","type":"note","number":2}])");
    feedback.setSink (&sink);
    state.values["a"] = 1.0;
    EXPECT_EQ (feedback.update(), 1);
    EXPECT_EQ (feedback.update(), 0);

    load (R"([{"source":"a","type":"note","number":3}])");
    EXPECT_EQ (feedback.update(), 1) << "new rules light up from scratch";
    EXPECT_EQ (sink.sent.back().getNoteNumber(), 3);

    mapper.setOutputs ({});
    EXPECT_EQ (feedback.update(), 0);
}

TEST_F (MidiFeedbackTest, ANewSinkGetsTheCurrentStateAgain)
{
    load (R"([{"source":"a","type":"note","number":2}])");
    state.values["a"] = 1.0;
    feedback.setSink (&sink);
    feedback.update();
    RecordingOutputSink second;
    feedback.setSink (&second);
    EXPECT_EQ (feedback.update(), 1);
    EXPECT_EQ (second.sent.size(), 1u);
}

TEST (OutputRuleTest, ParseRejectsBadRulesAndRoundTrips)
{
    OutputRule r;
    EXPECT_TRUE (OutputRule::parse (juce::JSON::parse (R"({"type":"note"})"), r).failed()) << "no source";
    EXPECT_TRUE (OutputRule::parse (juce::JSON::parse (R"({"source":"a","type":"x"})"), r).failed());
    EXPECT_TRUE (OutputRule::parse (juce::JSON::parse (R"({"source":"a","type":"cc","number":200})"), r).failed());
    EXPECT_TRUE (OutputRule::parse (juce::JSON::parse (R"({"source":"a","type":"cc","blink":"x"})"), r).failed());
    EXPECT_TRUE (OutputRule::parse (juce::JSON::parse (R"({"source":"a","type":"cc","mode":"x"})"), r).failed());
    ASSERT_TRUE (OutputRule::parse (juce::JSON::parse (R"({"source":"a","type":"cc","number":5,"on":90,"blink":"on"})"), r).wasOk());
    OutputRule again;
    ASSERT_TRUE (OutputRule::parse (r.toVar(), again).wasOk());
    EXPECT_EQ (again.onValue, 90);
    EXPECT_EQ (again.blink, OutputRule::Blink::WhenOn);
    EXPECT_EQ (again.number, 5);
}

TEST (MidiOutputRouterTest, PairsOutputsToEnabledInputsByName)
{
    juce::Array<juce::MidiDeviceInfo> in { { "Pad", "in1" }, { "Deck", "in2" } };
    juce::Array<juce::MidiDeviceInfo> out { { "Deck", "out1" }, { "Other", "out2" }, { "Pad", "out3" } };
    const auto some = MidiOutputRouter::pairOutputs (in, { "in2" }, false, out);
    ASSERT_EQ (some.size(), 1);
    EXPECT_EQ (some[0].identifier, "out1");
    EXPECT_EQ (MidiOutputRouter::pairOutputs (in, {}, true, out).size(), 2);
    EXPECT_EQ (MidiOutputRouter::pairOutputs (in, {}, false, out).size(), 0);
}

TEST (EngineStateReaderTest, ReportsDeckAndMixerStateByIdAndRejectsUnknownIds)
{
    DjEngine engine;
    EngineStateReader reader (engine);
    double v = -1.0;
    ASSERT_TRUE (reader.readState ("deck.0.play", v));
    EXPECT_EQ (v, 0.0);
    engine.getDeck (0).play();
    ASSERT_TRUE (reader.readState ("deck.0.play", v));
    EXPECT_EQ (v, 1.0);

    engine.getMixer().setPfl (1, true);
    ASSERT_TRUE (reader.readState ("mixer.ch1.pfl", v));
    EXPECT_EQ (v, 1.0);
    engine.getDeckControl (0).setSlip (true);
    ASSERT_TRUE (reader.readState ("deck.0.slip", v));
    EXPECT_EQ (v, 1.0);
    engine.getDeck (1).setHotcue (2, 1000);
    ASSERT_TRUE (reader.readState ("deck.1.hotcue2.set", v));
    EXPECT_EQ (v, 1.0);
    ASSERT_TRUE (reader.readState ("deck.1.hotcue3.set", v));
    EXPECT_EQ (v, 0.0);
    engine.getSync().setLeader (1);
    ASSERT_TRUE (reader.readState ("deck.1.sync.master", v));
    EXPECT_EQ (v, 1.0);
    for (const char* id : { "mixer.master.level", "mixer.crossfader", "mixer.ch0.level", "deck.0.cue", "deck.0.loop", "deck.0.position" })
        EXPECT_TRUE (reader.readState (id, v)) << id;

    for (const char* id : { "", "deck", "deck.99.play", "deck.0.bogus", "deck.x.play", "mixer.ch99.pfl", "mixer.ch0.bogus", "nope.0.play", "deck.0.hotcuex.set" })
        EXPECT_FALSE (reader.readState (id, v)) << id;
}
