#include <JuceHeader.h>
#include "controllers/MidiMapper.h"
#include "controllers/MidiTrace.h"
#include "controllers/EngineTarget.h"

namespace OpenMixerTests
{
    class MidiMapperTest : public juce::UnitTest
    {
    public:
        MidiMapperTest() : juce::UnitTest ("MIDI Mapper", "OpenMixer Controllers") {}

        static juce::File dataDir() { return juce::File (OPENMIXER_TEST_DATA_DIR).getChildFile ("controllers"); }

        void runTest() override
        {
            using namespace openmixer;

            beginTest ("Rejects invalid maps without changing state");
            {
                MidiMapper m;
                expect (m.loadFromJson ("not json").failed());
                expect (m.loadFromJson ("{}").failed());
                expect (m.loadFromJson (R"({"mappings":[{"type":"cc","number":1}]})").failed(), "missing target");
                expect (m.loadFromJson (R"({"mappings":[{"type":"zzz","number":1,"target":"a"}]})").failed());
                expect (m.loadFromJson (R"({"mappings":[{"type":"cc","number":200,"target":"a"}]})").failed());
                expect (m.loadFromJson (R"({"mappings":[{"type":"cc","number":1,"target":"a","mode":"x"}]})").failed());
                expect (m.loadFromJson (R"({"mappings":[{"type":"cc","number":1,"target":"a","min":1,"max":1}]})").failed());
                expectEquals ((int) m.getNumMappings(), 0);
                expect (m.loadFromFile (dataDir().getChildFile ("missing.json")).failed());
            }

            beginTest ("Absolute CC scales, inverts and respects channel filter");
            {
                MidiMapper m; RecordingTarget t; m.setTarget (&t);
                expect (m.loadFromJson (R"({"mappings":[
                    {"type":"cc","channel":2,"number":7,"target":"a","min":10,"max":20},
                    {"type":"cc","number":8,"target":"b","invert":true}]})").wasOk());
                expectEquals (m.handleMessage (juce::MidiMessage::controllerEvent (1, 7, 127)), 0);   // wrong channel
                expectEquals (m.handleMessage (juce::MidiMessage::controllerEvent (2, 7, 127)), 1);
                expectWithinAbsoluteError (t.last ("a"), 20.0, 1e-9);
                m.handleMessage (juce::MidiMessage::controllerEvent (2, 7, 0));
                expectWithinAbsoluteError (t.last ("a"), 10.0, 1e-9);
                m.handleMessage (juce::MidiMessage::controllerEvent (9, 8, 127));   // any channel
                expectWithinAbsoluteError (t.last ("b"), 0.0, 1e-9);
                expectWithinAbsoluteError (m.getValue ("b"), 0.0, 1e-9);
            }

            beginTest ("Relative modes accumulate and clamp");
            {
                MidiMapper m; RecordingTarget t; m.setTarget (&t);
                m.loadFromJson (R"({"mappings":[
                    {"type":"cc","number":1,"target":"r","mode":"relative","step":0.1,"min":0,"max":1,"initial":0.5},
                    {"type":"cc","number":2,"target":"c","mode":"relative2c","step":0.1,"min":0,"max":1,"initial":0.5}]})");
                for (int i = 0; i < 3; ++i) m.handleMessage (juce::MidiMessage::controllerEvent (1, 1, 65));
                expectWithinAbsoluteError (t.last ("r"), 0.8, 1e-9);
                for (int i = 0; i < 100; ++i) m.handleMessage (juce::MidiMessage::controllerEvent (1, 1, 127));
                expectWithinAbsoluteError (t.last ("r"), 1.0, 1e-9);
                m.handleMessage (juce::MidiMessage::controllerEvent (1, 2, 127));   // -1 in two's complement
                expectWithinAbsoluteError (t.last ("c"), 0.4, 1e-9);
                m.handleMessage (juce::MidiMessage::controllerEvent (1, 2, 2));
                expectWithinAbsoluteError (t.last ("c"), 0.6, 1e-9);
            }

            beginTest ("Button, toggle and pitch bend");
            {
                MidiMapper m; RecordingTarget t; m.setTarget (&t);
                m.loadFromJson (R"({"mappings":[
                    {"type":"note","number":60,"target":"btn"},
                    {"type":"note","number":61,"target":"tog","mode":"toggle"},
                    {"type":"pitchbend","target":"pb","min":-1,"max":1}]})");
                m.handleMessage (juce::MidiMessage::noteOn (1, 60, (juce::uint8) 100));
                expectEquals (t.last ("btn"), 1.0);
                m.handleMessage (juce::MidiMessage::noteOff (1, 60));
                expectEquals (t.last ("btn"), 0.0);
                m.handleMessage (juce::MidiMessage::noteOn (1, 61, (juce::uint8) 100));
                m.handleMessage (juce::MidiMessage::noteOff (1, 61));
                expectEquals (t.count ("tog"), 1);          // release ignored
                expectEquals (t.last ("tog"), 1.0);
                m.handleMessage (juce::MidiMessage::noteOn (1, 61, (juce::uint8) 100));
                expectEquals (t.last ("tog"), 0.0);
                m.handleMessage (juce::MidiMessage::pitchWheel (1, 16383));
                expectWithinAbsoluteError (t.last ("pb"), 1.0, 1e-9);
                m.handleMessage (juce::MidiMessage::pitchWheel (1, 0));
                expectWithinAbsoluteError (t.last ("pb"), -1.0, 1e-9);
                expectEquals (m.handleMessage (juce::MidiMessage::programChange (1, 3)), 0);   // unsupported type
            }

            beginTest ("Trace replay of a recorded stream through a map file");
            {
                MidiMapper m; RecordingTarget t; m.setTarget (&t);
                auto r = m.loadFromFile (dataDir().getChildFile ("test_map.json"));
                expect (r.wasOk(), r.getErrorMessage());
                expectEquals ((int) m.getNumMappings(), 7);

                MidiTrace trace;
                r = MidiTrace::fromFile (dataDir().getChildFile ("test_trace.json"), trace);
                expect (r.wasOk(), r.getErrorMessage());
                expectEquals ((int) trace.events.size(), 12);

                const int fired = replayTrace (trace, m);
                // 2 fader + 3 eq + 2 toggle presses + 2 pitch bend; note-offs on toggle and the
                // channel-6 CC match nothing.
                expectEquals (fired, 9);
                expectEquals (t.count ("mixer.ch0.fader"), 2);
                expectWithinAbsoluteError (t.last ("mixer.ch0.fader"), 1.0, 1e-9);
                expectWithinAbsoluteError (t.last ("mixer.ch0.eq.low"), 0.5, 1e-9);   // +0.5 +0.5 -0.5
                expectEquals (t.count ("deck.0.play"), 2);
                expectEquals (t.last ("deck.0.play", -1.0), 0.0);                     // toggled on, then off
                expectWithinAbsoluteError (t.last ("deck.0.rate"), 0.92, 1e-9);
                expectEquals (t.count ("mixer.ch0.pfl"), 0);
            }

            beginTest ("Trace JSON round-trips");
            {
                MidiTrace a;
                a.name = "rt";
                a.add (0.0, juce::MidiMessage::controllerEvent (3, 11, 99));
                a.add (0.5, juce::MidiMessage::noteOn (1, 36, (juce::uint8) 127));
                MidiTrace b;
                expect (MidiTrace::fromJson (a.toJson(), b).wasOk());
                expectEquals ((int) b.events.size(), 2);
                expectEquals (b.name, a.name);
                expect (b.events[0].message.isController());
                expectEquals (b.events[0].message.getControllerValue(), 99);
                expectWithinAbsoluteError (b.events[1].time, 0.5, 1e-9);
                expect (MidiTrace::fromJson ("{\"events\":[{\"bytes\":\"B0 07\"}]}", b).failed(), "truncated message");
            }

            beginTest ("EngineTarget drives the Mixer from a replayed trace");
            {
                Mixer mixer (2);
                mixer.prepare (48000.0, 256);
                EngineTarget engine (mixer, {});
                MidiMapper m; m.setTarget (&engine);
                expect (m.loadFromFile (dataDir().getChildFile ("test_map.json")).wasOk());
                MidiTrace trace;
                expect (MidiTrace::fromFile (dataDir().getChildFile ("test_trace.json"), trace).wasOk());
                replayTrace (trace, m);
                expectWithinAbsoluteError (mixer.getFader (0), 1.0f, 1e-6f);
                expectWithinAbsoluteError (mixer.getEq (0, 0), 0.5f, 1e-6f);
                expectEquals (engine.getUnknownCount(), 4);   // 2 play + 2 rate, but no decks attached

                m.handleMessage (juce::MidiMessage::noteOn (1, 40, (juce::uint8) 127));
                expect (mixer.getPfl (0));
                m.handleMessage (juce::MidiMessage::noteOff (1, 40));
                expect (! mixer.getPfl (0));
                m.handleMessage (juce::MidiMessage::controllerEvent (1, 8, 127));   // inverted ch1 fader
                expectWithinAbsoluteError (mixer.getFader (1), 0.0f, 1e-6f);

                engine.setParameter ("mixer.nonsense", 1.0);
                expectEquals (engine.getUnknownCount(), 5);
            }
        }
    };

    MidiMapperTest midiMapperTest;
}
