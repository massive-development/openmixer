// OpenMixer MidiMapperTest - MIDI map parsing, value modes, trace replay and parameter routing
#include <JuceHeader.h>
#include "controllers/MidiMapper.h"
#include "controllers/MidiTrace.h"
#include "controllers/ParameterRouter.h"
#include "controllers/RecordingTarget.h"
#include "engine/Deck.h"
#include "engine/Mixer.h"

#include <gtest/gtest.h>

using namespace openmixer;

namespace
{
juce::File dataDir() { return juce::File (OPENMIXER_TEST_DATA_DIR).getChildFile ("controllers"); }

juce::uint8 u8 (int v) { return static_cast<juce::uint8> (v); }
}

class MidiMapperTest : public ::testing::Test
{
protected:
    MidiMapperTest() { m.setTarget (&t); }

    MidiMapper m;
    RecordingTarget t;
};

TEST_F (MidiMapperTest, RejectsInvalidMapsWithoutChangingState)
{
    EXPECT_TRUE (m.loadFromJson ("not json").failed());
    EXPECT_TRUE (m.loadFromJson ("{}").failed());
    EXPECT_TRUE (m.loadFromJson (R"({"mappings":[{"type":"cc","number":1}]})").failed()) << "missing target";
    EXPECT_TRUE (m.loadFromJson (R"({"mappings":[{"type":"zzz","number":1,"target":"a"}]})").failed());
    EXPECT_TRUE (m.loadFromJson (R"({"mappings":[{"type":"cc","number":200,"target":"a"}]})").failed());
    EXPECT_TRUE (m.loadFromJson (R"({"mappings":[{"type":"cc","number":1,"target":"a","mode":"x"}]})").failed());
    EXPECT_TRUE (m.loadFromJson (R"({"mappings":[{"type":"cc","number":1,"target":"a","min":1,"max":1}]})").failed());
    EXPECT_EQ (static_cast<int> (m.getNumMappings()), 0);
    EXPECT_TRUE (m.loadFromFile (dataDir().getChildFile ("missing.json")).failed());
}

TEST_F (MidiMapperTest, AbsoluteCcScalesInvertsAndRespectsChannelFilter)
{
    ASSERT_TRUE (m.loadFromJson (R"({"mappings":[
        {"type":"cc","channel":2,"number":7,"target":"a","min":10,"max":20},
        {"type":"cc","number":8,"target":"b","invert":true}]})").wasOk());
    EXPECT_EQ (m.handleMessage (juce::MidiMessage::controllerEvent (1, 7, 127)), 0);   // wrong channel
    EXPECT_EQ (m.handleMessage (juce::MidiMessage::controllerEvent (2, 7, 127)), 1);
    EXPECT_NEAR (t.last ("a"), 20.0, 1e-9);
    m.handleMessage (juce::MidiMessage::controllerEvent (2, 7, 0));
    EXPECT_NEAR (t.last ("a"), 10.0, 1e-9);
    m.handleMessage (juce::MidiMessage::controllerEvent (9, 8, 127));   // any channel
    EXPECT_NEAR (t.last ("b"), 0.0, 1e-9);
    EXPECT_NEAR (m.getValue ("b"), 0.0, 1e-9);
}

TEST_F (MidiMapperTest, RelativeModesAccumulateAndClamp)
{
    m.loadFromJson (R"({"mappings":[
        {"type":"cc","number":1,"target":"r","mode":"relative","step":0.1,"min":0,"max":1,"initial":0.5},
        {"type":"cc","number":2,"target":"c","mode":"relative2c","step":0.1,"min":0,"max":1,"initial":0.5}]})");
    for (int i = 0; i < 3; ++i) m.handleMessage (juce::MidiMessage::controllerEvent (1, 1, 65));
    EXPECT_NEAR (t.last ("r"), 0.8, 1e-9);
    for (int i = 0; i < 100; ++i) m.handleMessage (juce::MidiMessage::controllerEvent (1, 1, 127));
    EXPECT_NEAR (t.last ("r"), 1.0, 1e-9);
    m.handleMessage (juce::MidiMessage::controllerEvent (1, 2, 127));   // -1 in two's complement
    EXPECT_NEAR (t.last ("c"), 0.4, 1e-9);
    m.handleMessage (juce::MidiMessage::controllerEvent (1, 2, 2));
    EXPECT_NEAR (t.last ("c"), 0.6, 1e-9);
}

TEST_F (MidiMapperTest, ButtonToggleAndPitchBend)
{
    m.loadFromJson (R"({"mappings":[
        {"type":"note","number":60,"target":"btn"},
        {"type":"note","number":61,"target":"tog","mode":"toggle"},
        {"type":"pitchbend","target":"pb","min":-1,"max":1}]})");
    m.handleMessage (juce::MidiMessage::noteOn (1, 60, u8 (100)));
    EXPECT_EQ (t.last ("btn"), 1.0);
    m.handleMessage (juce::MidiMessage::noteOff (1, 60));
    EXPECT_EQ (t.last ("btn"), 0.0);
    m.handleMessage (juce::MidiMessage::noteOn (1, 61, u8 (100)));
    m.handleMessage (juce::MidiMessage::noteOff (1, 61));
    EXPECT_EQ (t.count ("tog"), 1);   // release ignored
    EXPECT_EQ (t.last ("tog"), 1.0);
    m.handleMessage (juce::MidiMessage::noteOn (1, 61, u8 (100)));
    EXPECT_EQ (t.last ("tog"), 0.0);
    m.handleMessage (juce::MidiMessage::pitchWheel (1, 16383));
    EXPECT_NEAR (t.last ("pb"), 1.0, 1e-9);
    m.handleMessage (juce::MidiMessage::pitchWheel (1, 0));
    EXPECT_NEAR (t.last ("pb"), -1.0, 1e-9);
    EXPECT_EQ (m.handleMessage (juce::MidiMessage::programChange (1, 3)), 0);   // unsupported type
}

TEST_F (MidiMapperTest, TraceReplayOfARecordedStreamThroughAMapFile)
{
    auto r = m.loadFromFile (dataDir().getChildFile ("test_map.json"));
    ASSERT_TRUE (r.wasOk()) << r.getErrorMessage();
    EXPECT_EQ (static_cast<int> (m.getNumMappings()), 7);

    MidiTrace trace;
    r = MidiTrace::fromFile (dataDir().getChildFile ("test_trace.json"), trace);
    ASSERT_TRUE (r.wasOk()) << r.getErrorMessage();
    EXPECT_EQ (static_cast<int> (trace.events.size()), 12);

    const int fired = replayTrace (trace, m);
    // 2 fader + 3 eq + 2 toggle presses + 2 pitch bend; note-offs on toggle and the
    // channel-6 CC match nothing.
    EXPECT_EQ (fired, 9);
    EXPECT_EQ (t.count ("mixer.ch0.fader"), 2);
    EXPECT_NEAR (t.last ("mixer.ch0.fader"), 1.0, 1e-9);
    EXPECT_NEAR (t.last ("mixer.ch0.eq.low"), 0.5, 1e-9);   // +0.5 +0.5 -0.5
    EXPECT_EQ (t.count ("deck.0.play"), 2);
    EXPECT_EQ (t.last ("deck.0.play", -1.0), 0.0);          // toggled on, then off
    EXPECT_NEAR (t.last ("deck.0.rate"), 0.92, 1e-9);
    EXPECT_EQ (t.count ("mixer.ch0.pfl"), 0);
}

TEST (MidiTraceTest, JsonRoundTrips)
{
    MidiTrace a;
    a.name = "rt";
    a.add (0.0, juce::MidiMessage::controllerEvent (3, 11, 99));
    a.add (0.5, juce::MidiMessage::noteOn (1, 36, u8 (127)));
    MidiTrace b;
    ASSERT_TRUE (MidiTrace::fromJson (a.toJson(), b).wasOk());
    ASSERT_EQ (static_cast<int> (b.events.size()), 2);
    EXPECT_EQ (b.name, a.name);
    EXPECT_TRUE (b.events[0].message.isController());
    EXPECT_EQ (b.events[0].message.getControllerValue(), 99);
    EXPECT_NEAR (b.events[1].time, 0.5, 1e-9);
    EXPECT_TRUE (MidiTrace::fromJson ("{\"events\":[{\"bytes\":\"B0 07\"}]}", b).failed()) << "truncated message";
}

TEST_F (MidiMapperTest, ParameterRouterDrivesMixerAndDeckFromAReplayedTrace)
{
    Mixer mixer (2);
    mixer.prepare (48000.0, 256);
    Deck deck;
    ParameterRouter router;
    router.registerTarget ("mixer", &mixer);
    router.registerTarget ("deck.0", &deck);
    m.setTarget (&router);
    ASSERT_TRUE (m.loadFromFile (dataDir().getChildFile ("test_map.json")).wasOk());
    MidiTrace trace;
    ASSERT_TRUE (MidiTrace::fromFile (dataDir().getChildFile ("test_trace.json"), trace).wasOk());
    replayTrace (trace, m);
    EXPECT_NEAR (mixer.getFader (0), 1.0f, 1e-6f);
    EXPECT_NEAR (mixer.getEq (0, 0), 0.5f, 1e-6f);
    EXPECT_NEAR (deck.getRate(), 0.92, 1e-9);
    EXPECT_FALSE (deck.isPlaying()) << "toggled on then off";
    EXPECT_EQ (router.getUnknownCount(), 0);

    m.handleMessage (juce::MidiMessage::noteOn (1, 40, u8 (127)));
    EXPECT_TRUE (mixer.getPfl (0));
    m.handleMessage (juce::MidiMessage::noteOff (1, 40));
    EXPECT_FALSE (mixer.getPfl (0));
    m.handleMessage (juce::MidiMessage::controllerEvent (1, 8, 127));   // inverted ch1 fader
    EXPECT_NEAR (mixer.getFader (1), 0.0f, 1e-6f);

    EXPECT_FALSE (router.setParameter ("mixer.nonsense", 1.0));
    EXPECT_FALSE (router.setParameter ("nobody.home", 1.0));
    EXPECT_EQ (router.getUnknownCount(), 2);
}

TEST (ParameterRouterTest, UserTargetsCanBeRegisteredReplacedAndRemoved)
{
    RecordingTarget a, b;
    ParameterRouter router;
    router.registerTarget ("fx", &a);
    router.registerTarget ("fx.delay", &b);   // longer prefix wins
    EXPECT_TRUE (router.setParameter ("fx.delay.time", 0.3));
    EXPECT_EQ (b.last ("time"), 0.3);
    EXPECT_TRUE (router.setParameter ("fx.reverb.size", 0.7));
    EXPECT_EQ (a.last ("reverb.size"), 0.7);
    router.unregisterTarget ("fx.delay");
    EXPECT_TRUE (router.setParameter ("fx.delay.time", 0.9));
    EXPECT_EQ (a.last ("delay.time"), 0.9);
    EXPECT_FALSE (router.setParameter ("fxx.time", 1.0)) << "prefix must match a whole segment";
}
