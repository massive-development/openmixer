// OpenMixer MidiLearnTest - learn mode per control type, relative/14-bit decoding, map v2 save/load, binding edits
#include <JuceHeader.h>
#include "controllers/MidiMapper.h"
#include "controllers/MidiTrace.h"
#include "controllers/RecordingTarget.h"

#include <gtest/gtest.h>

using namespace openmixer;

namespace
{
MidiTrace traceOf (std::initializer_list<const char*> hexMessages)
{
    MidiTrace t;
    double time = 0.0;
    for (const auto* hex : hexMessages)
    {
        juce::MemoryBlock block;
        block.loadFromHexString (hex);
        t.add (time += 0.01, juce::MidiMessage (block.getData(), static_cast<int> (block.getSize())));
    }
    return t;
}

LearnRequest request (const char* target, LearnType type = LearnType::Auto)
{
    LearnRequest r;
    r.target = target;
    r.type = type;
    return r;
}
}

class MidiLearnTest : public ::testing::Test
{
protected:
    MidiLearnTest()
    {
        m.setTarget (&t);
        m.setOnLearned ([this] (const Mapping& mapping) { learned.push_back (mapping); });
    }

    // Arms and replays a one-message trace; returns the new binding.
    Mapping learn (const LearnRequest& r, const char* hex)
    {
        m.armLearn (r);
        replayTrace (traceOf ({ hex }), m);
        return m.getBindings().empty() ? Mapping() : m.getBindings().back();
    }

    MidiMapper m;
    RecordingTarget t;
    std::vector<Mapping> learned;
};

TEST_F (MidiLearnTest, CcBecomesAbsoluteOnItsChannelAndIsNotForwarded)
{
    const auto b = learn (request ("mixer.ch0.fader"), "B2 07 40");
    EXPECT_EQ (b.kind, MidiEvent::Kind::CC);
    EXPECT_EQ (b.channel, 3);
    EXPECT_EQ (b.number, 7);
    EXPECT_EQ (b.mode, MapMode::Absolute);
    EXPECT_TRUE (t.changes.empty()) << "the learning message is consumed";
    EXPECT_FALSE (m.isLearning());
    ASSERT_EQ (learned.size(), 1u);
    EXPECT_EQ (learned[0].target, "mixer.ch0.fader");

    replayTrace (traceOf ({ "B2 07 7F", "B2 07 00" }), m);
    EXPECT_EQ (t.count ("mixer.ch0.fader"), 2);
    EXPECT_NEAR (t.last ("mixer.ch0.fader"), 0.0, 1e-9);
}

TEST_F (MidiLearnTest, AnyChannelRequestBindsAllChannels)
{
    auto r = request ("a");
    r.anyChannel = true;
    EXPECT_EQ (learn (r, "B4 10 01").channel, 0);
    replayTrace (traceOf ({ "B9 10 7F" }), m);
    EXPECT_EQ (t.count ("a"), 1);
}

TEST_F (MidiLearnTest, NoteBecomesButtonOrToggleAndIgnoresNoteOff)
{
    m.armLearn (request ("deck.0.hotcue0"));
    replayTrace (traceOf ({ "80 24 00" }), m);   // a release alone cannot bind
    EXPECT_TRUE (m.isLearning());
    replayTrace (traceOf ({ "90 24 64" }), m);
    ASSERT_EQ (m.getBindings().size(), 1u);
    EXPECT_EQ (m.getBindings()[0].mode, MapMode::Button);

    replayTrace (traceOf ({ "90 24 64", "80 24 00" }), m);
    EXPECT_NEAR (t.last ("deck.0.hotcue0"), 1.0, 1e-9);
    EXPECT_EQ (t.count ("deck.0.hotcue0"), 2);

    const auto toggle = learn (request ("deck.0.play", LearnType::Toggle), "90 25 64");
    EXPECT_EQ (toggle.mode, MapMode::Toggle);
    t.changes.clear();
    replayTrace (traceOf ({ "90 25 64", "80 25 00", "90 25 64", "80 25 00" }), m);
    ASSERT_EQ (t.changes.size(), 2u);
    EXPECT_NEAR (t.changes[0].value, 1.0, 1e-9);
    EXPECT_NEAR (t.changes[1].value, 0.0, 1e-9);
}

TEST_F (MidiLearnTest, PitchBendBecomesAbsoluteWithTheRequestedRange)
{
    auto r = LearnRequest::forTarget ("deck.0.rate");
    const auto b = learn (r, "E0 7F 7F");
    EXPECT_EQ (b.kind, MidiEvent::Kind::PitchBend);
    EXPECT_NEAR (b.min, 0.92, 1e-9);
    replayTrace (traceOf ({ "E0 7F 7F", "E0 00 00" }), m);
    EXPECT_NEAR (t.last ("deck.0.rate"), 0.92, 1e-9);
    EXPECT_EQ (t.count ("deck.0.rate"), 2);
}

TEST_F (MidiLearnTest, OffsetEncoderDecodesAroundSixtyFour)
{
    auto r = request ("mixer.ch0.eq.low", LearnType::Relative);
    r.min = -48.0; r.max = 6.0; r.step = 0.5;
    EXPECT_EQ (learn (r, "B0 15 41").mode, MapMode::Relative);
    replayTrace (traceOf ({ "B0 15 41", "B0 15 42", "B0 15 3F" }), m);   // +1, +2, -1
    EXPECT_NEAR (m.getValue ("mixer.ch0.eq.low"), -48.0 + 1.0 + 0.5 * 2 - 0.5 + 0.5 * 0, 1e-9 + 0.5);
    ASSERT_EQ (t.changes.size(), 3u);
    EXPECT_NEAR (t.changes[1].value - t.changes[0].value, 1.0, 1e-9);
    EXPECT_NEAR (t.changes[2].value - t.changes[1].value, -0.5, 1e-9);
}

TEST_F (MidiLearnTest, TwosComplementEncoderDecodesNegativeAsHighValues)
{
    auto r = request ("mixer.crossfader", LearnType::RelativeTwosComplement);
    r.step = 0.1; r.initial = 0.5;
    EXPECT_EQ (learn (r, "B0 16 01").mode, MapMode::RelativeTwosComplement);
    t.changes.clear();
    replayTrace (traceOf ({ "B0 16 03", "B0 16 7E" }), m);   // +3, -2
    ASSERT_EQ (t.changes.size(), 2u);
    EXPECT_NEAR (t.changes[0].value, 0.8, 1e-9);
    EXPECT_NEAR (t.changes[1].value, 0.6, 1e-9);
}

TEST_F (MidiLearnTest, FourteenBitPairCombinesMsbAndLsb)
{
    const auto b = learn (request ("mixer.ch0.fader", LearnType::Absolute14), "B0 01 00");
    EXPECT_EQ (b.mode, MapMode::Absolute14);
    EXPECT_EQ (b.number, 1);
    EXPECT_EQ (b.lsbNumber, 33);
    t.changes.clear();
    replayTrace (traceOf ({ "B0 01 40", "B0 21 00" }), m);   // 64 << 7 = 8192
    EXPECT_NEAR (t.last ("mixer.ch0.fader"), 8192.0 / 16383.0, 1e-9);
    replayTrace (traceOf ({ "B0 01 7F", "B0 21 7F" }), m);
    EXPECT_NEAR (t.last ("mixer.ch0.fader"), 1.0, 1e-9);
    replayTrace (traceOf ({ "B0 01 00", "B0 21 01" }), m);
    EXPECT_NEAR (t.last ("mixer.ch0.fader"), 1.0 / 16383.0, 1e-9);
}

TEST_F (MidiLearnTest, FourteenBitAcceptsTheLsbFirstAndRefusesOutOfRangeControllers)
{
    m.armLearn (request ("x", LearnType::Absolute14));
    replayTrace (traceOf ({ "B0 40 10" }), m);   // cc 64 has no pair
    EXPECT_TRUE (m.isLearning());
    replayTrace (traceOf ({ "B0 22 10" }), m);   // LSB of cc 2
    ASSERT_EQ (m.getBindings().size(), 1u);
    EXPECT_EQ (m.getBindings()[0].number, 2);
}

TEST_F (MidiLearnTest, RelearnReplacesThePreviousBindingsOfTargetAndControl)
{
    learn (request ("a"), "B0 07 00");
    learn (request ("b"), "B0 08 00");
    learn (request ("a"), "B0 09 00");   // a moves to cc 9
    learn (request ("c"), "B0 08 00");   // cc 8 now drives c, not b
    const auto list = m.getBindings();
    ASSERT_EQ (list.size(), 2u);
    EXPECT_EQ (list[0].target, "a");
    EXPECT_EQ (list[0].number, 9);
    EXPECT_EQ (list[1].target, "c");
}

TEST_F (MidiLearnTest, CancelDisarmsAndLetsMessagesThrough)
{
    ASSERT_TRUE (m.addBinding ([] { Mapping x; x.number = 7; x.target = "a"; return x; }()).wasOk());
    m.armLearn (request ("b"));
    EXPECT_TRUE (m.isLearning());
    m.cancelLearn();
    EXPECT_FALSE (m.isLearning());
    replayTrace (traceOf ({ "B0 07 7F" }), m);
    EXPECT_EQ (t.count ("a"), 1);
    EXPECT_TRUE (learned.empty());
}

TEST_F (MidiLearnTest, DefaultsFollowTheTargetId)
{
    EXPECT_EQ (LearnRequest::forTarget ("deck.0.play").type, LearnType::Toggle);
    EXPECT_EQ (LearnRequest::forTarget ("deck.1.hotcue3").type, LearnType::Button);
    EXPECT_EQ (LearnRequest::forTarget ("mixer.ch2.pfl").type, LearnType::Button);
    EXPECT_EQ (LearnRequest::forTarget ("deck.0.loop.exit").type, LearnType::Button);
    EXPECT_EQ (LearnRequest::forTarget ("mixer.ch0.fader").type, LearnType::Auto);
    const auto eq = LearnRequest::forTarget ("mixer.ch0.eq.high");
    EXPECT_EQ (eq.min, -48.0);
    EXPECT_EQ (eq.max, 6.0);
    EXPECT_EQ (LearnRequest::forTarget ("mixer.master").max, 2.0);
}

TEST_F (MidiLearnTest, SaveLoadRoundTripKeepsEverythingAtSchemaV2)
{
    learn (request ("mixer.ch0.fader", LearnType::Absolute14), "B0 01 00");
    auto r = request ("mixer.ch0.eq.low", LearnType::Relative);
    r.min = -48.0; r.max = 6.0; r.step = 0.5; r.invert = true;
    learn (r, "B1 15 41");
    learn (request ("deck.0.play", LearnType::Toggle), "90 24 64");
    OutputRule led;
    led.source = "deck.0.play"; led.number = 24; led.blink = OutputRule::Blink::WhenOff; led.blinkMs = 100;
    OutputRule meter;
    meter.source = "mixer.master.level"; meter.kind = MidiEvent::Kind::CC; meter.number = 9; meter.mode = OutputRule::Mode::Level;
    m.setOutputs ({ led, meter });
    m.setName ("Round trip");

    const auto file = juce::File::createTempFile (".json");
    ASSERT_TRUE (m.saveToFile (file).wasOk());
    EXPECT_EQ (juce::JSON::parse (file.loadFileAsString()).getProperty ("version", 0), juce::var (2));

    MidiMapper loaded;
    ASSERT_TRUE (loaded.loadFromFile (file).wasOk());
    EXPECT_EQ (loaded.getName(), "Round trip");
    EXPECT_EQ (loaded.getBindings(), m.getBindings());
    const auto outs = loaded.getOutputs();
    ASSERT_EQ (outs.size(), 2u);
    EXPECT_EQ (outs[0].blink, OutputRule::Blink::WhenOff);
    EXPECT_EQ (outs[0].blinkMs, 100);
    EXPECT_EQ (outs[1].mode, OutputRule::Mode::Level);
    EXPECT_EQ (loaded.toJson(), m.toJson());
    file.deleteFile();
}

TEST (MidiMapSchemaTest, VersionOneFilesWithoutAVersionFieldStillLoad)
{
    MidiMap map;
    ASSERT_TRUE (MidiMap::parse (R"({"name":"old","mappings":[{"type":"cc","number":7,"target":"a"}]})", map).wasOk());
    EXPECT_EQ (map.version, 1);
    EXPECT_EQ (map.mappings.size(), 1u);
    EXPECT_TRUE (map.outputs.empty());
    EXPECT_TRUE (MidiMap::parse (R"({"version":3,"mappings":[]})", map).failed());
    EXPECT_TRUE (MidiMap::parse (R"({"version":2,"mappings":[],"outputs":[{"source":"x","type":"zzz"}]})", map).failed());
    EXPECT_TRUE (MidiMap::parse (R"({"mappings":[{"type":"note","number":1,"target":"a","mode":"absolute14"}]})", map).failed());
}

TEST_F (MidiLearnTest, BindingEditingAddsReplacesRemovesAndValidates)
{
    Mapping a;
    a.number = 1; a.target = "a";
    Mapping b = a;
    b.number = 2; b.target = "b";
    ASSERT_TRUE (m.addBinding (a).wasOk());
    ASSERT_TRUE (m.addBinding (b).wasOk());
    Mapping bad = a;
    bad.target = {};
    EXPECT_TRUE (m.addBinding (bad).failed());
    bad = a; bad.max = bad.min;
    EXPECT_TRUE (m.replaceBinding (0, bad).failed());
    EXPECT_TRUE (m.replaceBinding (5, a).failed());
    EXPECT_EQ (m.getNumMappings(), 2u);

    a.number = 9;
    ASSERT_TRUE (m.replaceBinding (0, a).wasOk());
    replayTrace (traceOf ({ "B0 01 7F", "B0 09 7F" }), m);
    EXPECT_EQ (t.count ("a"), 1) << "old address is gone, the new one works";

    EXPECT_TRUE (m.removeBinding (1));
    EXPECT_FALSE (m.removeBinding (1));
    EXPECT_EQ (m.removeBindingsFor ("a"), 1);
    EXPECT_EQ (m.getNumMappings(), 0u);
}

TEST_F (MidiLearnTest, EveryEditBumpsTheGeneration)
{
    const auto g0 = m.getGeneration();
    Mapping a;
    a.target = "a";
    m.addBinding (a);
    const auto g1 = m.getGeneration();
    m.setOutputs ({});
    EXPECT_GT (g1, g0);
    EXPECT_GT (m.getGeneration(), g1);
}
