// OpenMixer PluginRegistryTest - plugin registration, stage attach/detach, MIDI maps and shared-library loading
#include <JuceHeader.h>
#include "plugins/PluginRegistry.h"

#include <gtest/gtest.h>

#include <vector>

using namespace openmixer;

namespace
{
constexpr int kBlock = 256;

struct Rig
{
    Mixer mixer { 2 };
    std::vector<float> in, mL, mR, cL, cR;

    Rig() : in (kBlock, 0.5F), mL (kBlock), mR (kBlock), cL (kBlock), cR (kBlock) { mixer.prepare (48000.0, kBlock); }

    float masterPeak()
    {
        Mixer::ChannelInput inputs[2] = { { in.data(), in.data() }, {} };
        for (int i = 0; i < 400; ++i)   // let smoothers and the peak decay settle
            mixer.process (inputs, mL.data(), mR.data(), cL.data(), cR.data(), kBlock);
        return mixer.getMasterPeakLeft();
    }
};

struct Recorder : ParameterTarget
{
    bool setParameter (const juce::String& id, double v) override { lastId = id; lastValue = v; return true; }
    juce::String lastId;
    double lastValue = 0.0;
};

struct Mute final : ProcessStage
{
    void process (float* l, float* r, int n) noexcept override
    {
        for (int i = 0; i < n; ++i) { l[i] = 0.0F; r[i] = 0.0F; }
    }
};

// In-process plugin so registry behaviour is testable without a shared library.
struct TestPlugin final : Plugin
{
    TestPlugin (juce::String pluginId, Recorder* r, int version = kPluginApiVersion)
        : id (std::move (pluginId)), recorder (r), api (version) {}

    int apiVersion() const noexcept override { return api; }
    PluginInfo info() const override { return { id, "Test", "1" }; }

    void onLoad (PluginHost& host) override
    {
        stageOk = host.registerStage ("test.mute", StageKind::PreProcess, [] { return std::make_unique<Mute>(); });
        duplicateStageOk = host.registerStage ("test.mute", StageKind::Fx, [] { return std::make_unique<Mute>(); });
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
}

TEST (PluginRegistryTest, RegistrationsAreValidatedAndListed)
{
    PluginRegistry reg;
    Recorder rec;
    auto plugin = std::make_unique<TestPlugin> ("test.a", &rec);
    auto* raw = plugin.get();
    ASSERT_TRUE (reg.addPlugin (std::move (plugin)).wasOk());
    EXPECT_TRUE (raw->stageOk && raw->mapOk && raw->targetOk);
    EXPECT_TRUE (! raw->duplicateStageOk && ! raw->badMapOk);
    EXPECT_EQ (reg.getStageIds().joinIntoString (","), juce::String ("test.mute"));
    EXPECT_EQ (reg.getMidiMapNames().joinIntoString (","), juce::String ("test.map"));
    EXPECT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).failed());
    EXPECT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.b", &rec, 99)).failed());
    EXPECT_TRUE (reg.addPlugin (nullptr).failed());
}

TEST (PluginRegistryTest, StagesAttachToChannelAndMasterThenDetachOnUnload)
{
    Recorder rec;
    Rig rig;
    PluginRegistry reg;   // declared last: it detaches from the rig on destruction
    const auto baseline = rig.masterPeak();
    ASSERT_GT (baseline, 0.0F);

    ASSERT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
    ASSERT_TRUE (reg.attachStage ("test.mute", rig.mixer, StageCatalog::kMasterChannel, 0).wasOk());
    EXPECT_LT (rig.masterPeak(), baseline * 0.01F);

    EXPECT_TRUE (reg.unload ("test.a"));
    EXPECT_GT (rig.masterPeak(), 0.0F);
    EXPECT_TRUE (reg.getStageIds().isEmpty());

    EXPECT_TRUE (reg.attachStage ("nope", rig.mixer, 0, 0).failed());
}

TEST (PluginRegistryTest, BadChannelOrSlotIsRejected)
{
    Recorder rec;
    Rig rig;
    PluginRegistry reg;
    ASSERT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
    EXPECT_TRUE (reg.attachStage ("test.mute", rig.mixer, 5, 0).failed());
    EXPECT_TRUE (reg.attachStage ("test.mute", rig.mixer, 0, StageChain::kMaxStages).failed());
    EXPECT_TRUE (reg.attachStage ("test.mute", rig.mixer, 0, 1).wasOk());
    reg.detachStage (rig.mixer, 0, 1);
}

TEST (PluginRegistryTest, ParameterTargetsFollowTheBoundRouter)
{
    ParameterRouter router;
    Recorder rec;
    PluginRegistry reg;
    ASSERT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
    reg.bindRouter (&router);
    EXPECT_TRUE (router.setParameter ("test.knob", 0.25));
    EXPECT_EQ (rec.lastId, juce::String ("knob"));
    EXPECT_EQ (rec.lastValue, 0.25);

    reg.unload ("test.a");
    EXPECT_FALSE (router.setParameter ("test.knob", 1.0));
}

TEST (PluginRegistryTest, MidiMapFromAPluginDrivesTheRouter)
{
    ParameterRouter router;
    Recorder rec;
    MidiMapper mapper;
    PluginRegistry reg;
    mapper.setTarget (&router);
    reg.bindRouter (&router);
    ASSERT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
    ASSERT_TRUE (reg.applyMidiMap ("test.map", mapper).wasOk());
    mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 7, 127));
    EXPECT_EQ (rec.lastId, juce::String ("x"));
    EXPECT_TRUE (reg.applyMidiMap ("missing", mapper).failed());
}

TEST (PluginRegistryTest, DestructionUnloadsPlugins)
{
    const auto before = TestPlugin::unloads;
    {
        PluginRegistry reg;
        Recorder rec;
        ASSERT_TRUE (reg.addPlugin (std::make_unique<TestPlugin> ("test.a", &rec)).wasOk());
    }
    EXPECT_EQ (TestPlugin::unloads, before + 1);
}

TEST (PluginLibraryTest, ExamplePluginLoadsFromASharedLibrary)
{
    const juce::File lib (OPENMIXER_EXAMPLE_PLUGIN_PATH);
    ParameterRouter router;
    MidiMapper mapper;
    Rig rig;
    PluginRegistry reg;
    mapper.setTarget (&router);
    reg.bindRouter (&router);

    ASSERT_TRUE (lib.existsAsFile());
    ASSERT_TRUE (reg.loadLibrary (lib).wasOk());
    EXPECT_TRUE (reg.loadLibrary (lib).failed());   // same id twice
    EXPECT_EQ (reg.getPluginIds().size(), 1);
    ASSERT_TRUE (reg.getStageIds().contains ("example.gain"));

    const auto baseline = rig.masterPeak();
    ASSERT_TRUE (reg.attachStage ("example.gain", rig.mixer, StageCatalog::kMasterChannel, 0).wasOk());
    EXPECT_NEAR (rig.masterPeak(), baseline, 1.0e-4F);

    ASSERT_TRUE (reg.applyMidiMap ("example.gain-knob", mapper).wasOk());
    mapper.handleMessage (juce::MidiMessage::controllerEvent (1, 1, 0));   // gain -> 0
    EXPECT_LT (rig.masterPeak(), baseline * 0.01F);

    EXPECT_TRUE (router.setParameter ("example.gain", 0.5));
    EXPECT_NEAR (rig.masterPeak(), baseline * 0.5F, 1.0e-3F);

    EXPECT_TRUE (reg.unload ("org.openmixer.example.gain"));
    EXPECT_FALSE (router.setParameter ("example.gain", 1.0));
    EXPECT_NEAR (rig.masterPeak(), baseline, 1.0e-4F);
}

TEST (PluginLibraryTest, InvalidLibrariesAreRejected)
{
    const juce::File lib (OPENMIXER_EXAMPLE_PLUGIN_PATH);
    PluginRegistry reg;
    EXPECT_TRUE (reg.loadLibrary (lib.getSiblingFile ("does_not_exist.so")).failed());

    const auto notLib = juce::File::getSpecialLocation (juce::File::tempDirectory).getChildFile ("om_not_a_plugin_" + juce::Uuid().toString() + ".so");
    ASSERT_TRUE (notLib.replaceWithText ("nope"));
    EXPECT_TRUE (reg.loadLibrary (notLib).failed());
    notLib.deleteFile();

    juce::StringArray errors;
    EXPECT_EQ (reg.loadDirectory (lib.getParentDirectory(), &errors), 1);
}
