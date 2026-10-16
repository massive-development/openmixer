#include <JuceHeader.h>
#include "plugins/PluginRegistry.h"

#include <cmath>
#include <vector>

namespace OpenMixerTests
{
    class PluginRegistryTest : public juce::UnitTest
    {
    public:
        PluginRegistryTest() : juce::UnitTest ("Plugin registry", "OpenMixer Plugins") {}

        static constexpr int kBlock = 256;

        struct Rig
        {
            openmixer::Mixer mixer { 2 };
            std::vector<float> in, mL, mR, cL, cR;

            Rig() : in (kBlock, 0.5F), mL (kBlock), mR (kBlock), cL (kBlock), cR (kBlock) { mixer.prepare (48000.0, kBlock); }

            float masterPeak()
            {
                openmixer::Mixer::ChannelInput inputs[2] = { { in.data(), in.data() }, {} };
                for (int i = 0; i < 400; ++i)   // let smoothers and the peak decay settle
                    mixer.process (inputs, mL.data(), mR.data(), cL.data(), cR.data(), kBlock);
                return mixer.getMasterPeakLeft();
            }
        };

        struct Recorder : openmixer::ParameterTarget
        {
            bool setParameter (const juce::String& id, double v) override { lastId = id; lastValue = v; return true; }
            juce::String lastId;
            double lastValue = 0.0;
        };

        struct Mute final : openmixer::ProcessStage
        {
            void process (float* l, float* r, int n) noexcept override
            {
                for (int i = 0; i < n; ++i) { l[i] = 0.0F; r[i] = 0.0F; }
            }
        };

        // In-process plugin so registry behaviour is testable without a shared library.
        struct TestPlugin final : openmixer::Plugin
        {
            TestPlugin (juce::String pluginId, Recorder* r, int version = openmixer::kPluginApiVersion)
                : id (std::move (pluginId)), recorder (r), api (version) {}

            int apiVersion() const noexcept override { return api; }
            openmixer::PluginInfo info() const override { return { id, "Test", "1" }; }

            void onLoad (openmixer::PluginHost& host) override
            {
                stageOk = host.registerStage ("test.mute", openmixer::StageKind::PreProcess, [] { return std::make_unique<Mute>(); });
                duplicateStageOk = host.registerStage ("test.mute", openmixer::StageKind::Fx, [] { return std::make_unique<Mute>(); });
                badMapOk = host.registerMidiMap ("bad", "{ not json");
                mapOk = host.registerMidiMap ("test.map", R"({ "mappings": [ { "type": "cc", "number": 7, "target": "test.x" } ] })");
                targetOk = host.registerParameterTarget ("test", recorder);
            }

            void onUnload() override { ++unloads; }

            juce::String id;
            Recorder* recorder;
            int api;
            bool stageOk = false, duplicateStageOk = true, badMapOk = true, mapOk = false, targetOk = false;
            static inline int unloads = 0;
        };

        void runTest() override
        {
            using namespace openmixer;

            beginTest ("Plugin registrations are validated and listed");
            {
                PluginRegistry reg;
                Recorder rec;
                auto plugin = std::make_unique<TestPlugin> ("test.a", &rec);
                auto* raw = plugin.get();
                expect (reg.addPlugin (std::move (plugin)).wasOk());
                expect (raw->stageOk && raw->mapOk && raw->targetOk);
                expect (! raw->duplicateStageOk && ! raw->badMapOk);
                expectEquals (reg.getStageIds().joinIntoString (","), juce::String ("test.mute"));
                expectEquals (reg.getMidiMapNames().joinIntoString (","), juce::String ("test.map"));
                expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).failed());
                expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.b", &rec, 99)).failed());
                expect (reg.addPlugin (nullptr).failed());
            }

            beginTest ("Stages attach to channel and master, then detach on unload");
            {
                Recorder rec;
                Rig rig;
                PluginRegistry reg;   // declared last: it detaches from the rig on destruction
                const auto baseline = rig.masterPeak();
                expectGreaterThan (baseline, 0.0F);

                expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
                expect (reg.attachStage ("test.mute", rig.mixer, StageCatalog::kMasterChannel, 0).wasOk());
                expectLessThan (rig.masterPeak(), baseline * 0.01F);

                expect (reg.unload ("test.a"));
                expectGreaterThan (rig.masterPeak(), 0.0F);
                expect (reg.getStageIds().isEmpty());

                expect (reg.attachStage ("nope", rig.mixer, 0, 0).failed());
            }

            beginTest ("Bad channel or slot is rejected");
            {
                Recorder rec;
                Rig rig;
                PluginRegistry reg;
                expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
                expect (reg.attachStage ("test.mute", rig.mixer, 5, 0).failed());
                expect (reg.attachStage ("test.mute", rig.mixer, 0, StageChain::kMaxStages).failed());
                expect (reg.attachStage ("test.mute", rig.mixer, 0, 1).wasOk());
                reg.detachStage (rig.mixer, 0, 1);
            }

            beginTest ("Parameter targets follow the bound router");
            {
                ParameterRouter router;
                Recorder rec;
                PluginRegistry reg;
                expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
                reg.bindRouter (&router);
                expect (router.setParameter ("test.knob", 0.25));
                expectEquals (rec.lastId, juce::String ("knob"));
                expectEquals (rec.lastValue, 0.25);

                reg.unload ("test.a");
                expect (! router.setParameter ("test.knob", 1.0));
            }

            beginTest ("MIDI map from a plugin drives the router");
            {
                ParameterRouter router;
                Recorder rec;
                MidiMapper mapper;
                PluginRegistry reg;
                mapper.setTarget (&router);
                reg.bindRouter (&router);
                expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
                expect (reg.applyMidiMap ("test.map", mapper).wasOk());
                mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 7, 127));
                expectEquals (rec.lastId, juce::String ("x"));
                expect (reg.applyMidiMap ("missing", mapper).failed());
            }

            beginTest ("Destruction unloads plugins");
            {
                const auto before = TestPlugin::unloads;
                {
                    PluginRegistry reg;
                    Recorder rec;
                    expect (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
                }
                expectEquals (TestPlugin::unloads, before + 1);
            }

            runLibraryTests();
        }

        void runLibraryTests()
        {
            using namespace openmixer;
            const juce::File lib (OPENMIXER_EXAMPLE_PLUGIN_PATH);

            beginTest ("Example plugin loads from a shared library");
            {
                ParameterRouter router;
                MidiMapper mapper;
                Rig rig;
                PluginRegistry reg;
                mapper.setTarget (&router);
                reg.bindRouter (&router);

                expect (lib.existsAsFile());
                expect (reg.loadLibrary (lib).wasOk());
                expect (reg.loadLibrary (lib).failed());   // same id twice
                expectEquals (reg.getPluginIds().size(), 1);
                expect (reg.getStageIds().contains ("example.gain"));

                const auto baseline = rig.masterPeak();
                expect (reg.attachStage ("example.gain", rig.mixer, StageCatalog::kMasterChannel, 0).wasOk());
                expectWithinAbsoluteError (rig.masterPeak(), baseline, 1.0e-4F);

                expect (reg.applyMidiMap ("example.gain-knob", mapper).wasOk());
                mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 1, 0));   // gain -> 0
                expectLessThan (rig.masterPeak(), baseline * 0.01F);

                expect (router.setParameter ("example.gain", 0.5));
                expectWithinAbsoluteError (rig.masterPeak(), baseline * 0.5F, 1.0e-3F);

                expect (reg.unload ("org.openmixer.example.gain"));
                expect (! router.setParameter ("example.gain", 1.0));
                expectWithinAbsoluteError (rig.masterPeak(), baseline, 1.0e-4F);
            }

            beginTest ("Invalid libraries are rejected");
            {
                PluginRegistry reg;
                expect (reg.loadLibrary (lib.getSiblingFile ("does_not_exist.so")).failed());
                const auto notLib = juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile ("om_not_a_plugin.so");
                notLib.replaceWithText ("nope");
                expect (reg.loadLibrary (notLib).failed());
                notLib.deleteFile();

                juce::StringArray errors;
                expectEquals (reg.loadDirectory (lib.getParentDirectory(), &errors), 1);
            }
        }
    };

    static PluginRegistryTest pluginRegistryTest;
}
