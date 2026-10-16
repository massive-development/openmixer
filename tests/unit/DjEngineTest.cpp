// OpenMixer DjEngineTest - engine routing of decks through the mixer to master and cue
#include <JuceHeader.h>
#include "engine/DjEngine.h"
#include "engine/MockAudioDevice.h"

#include <gtest/gtest.h>

#include <cmath>
#include <functional>
#include <thread>

using namespace openmixer;

namespace
{
constexpr int kFrames = 44100;
constexpr double kRate = 44100.0;

juce::File writeSine (const juce::File& f)
{
    f.deleteFile();
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), kRate, 2, 24, {}, 0));
    stream.release();   // writer owns the stream

    juce::AudioBuffer<float> buf (2, kFrames);
    for (int i = 0; i < kFrames; ++i)
        for (int c = 0; c < 2; ++c)
            buf.setSample (c, i, 0.5f * std::sin (2.0f * juce::MathConstants<float>::pi * 440.0f
                                                  * static_cast<float> (i) / static_cast<float> (kRate)));
    writer->writeFromAudioSampleBuffer (buf, 0, kFrames);
    return f;
}

/** Bounded poll: the decoder thread exposes no signal to wait on. */
bool pumpUntil (MockAudioDevice& dev, const std::function<bool()>& cond)
{
    const auto end = juce::Time::getMillisecondCounter() + 3000;
    while (! cond())
    {
        if (juce::Time::getMillisecondCounter() > end) return false;
        dev.renderBlock();
        std::this_thread::yield();
    }
    return true;
}

class DjEngineFixture : public ::testing::Test
{
protected:
    void SetUp() override
    {
        dir = juce::File::getSpecialLocation (juce::File::tempDirectory)
                  .getNonexistentChildFile ("openmixer_engine_test", {}, false);
        ASSERT_TRUE (dir.createDirectory());
        file = writeSine (dir.getChildFile ("sine.wav"));
    }

    void TearDown() override { dir.deleteRecursively(); }

    juce::File dir, file;
};
}

using DjEngineTest = DjEngineFixture;
using DjEngineSlow = DjEngineFixture;

TEST_F (DjEngineTest, SilentWhenNothingIsLoaded)
{
    DjEngine engine;
    MockAudioDevice dev (kRate, 256, 0, 4);
    dev.start (engine);
    dev.renderBlocks (4);
    for (int c = 0; c < 4; ++c)
        EXPECT_EQ (dev.getLastOutput().getMagnitude (c, 0, 256), 0.0f);
}

TEST_F (DjEngineSlow, DeckAudioReachesMasterPflReachesCueFaderGatesMaster)
{
    DjEngine engine;
    MockAudioDevice dev (kRate, 256, 0, 4);
    ASSERT_TRUE (engine.getDeck (0).load (file));
    dev.start (engine);
    engine.getMixer().setPfl (0, true);
    engine.getMixer().setFader (0, 0.0f);
    engine.getDeck (0).play();

    ASSERT_TRUE (pumpUntil (dev, [&] { return engine.getDeck (0).getPositionFrames() > 2000; }));
    EXPECT_LT (dev.getLastOutput().getMagnitude (0, 0, 256), 1e-6f) << "fader at zero silences master";
    EXPECT_GT (dev.getLastOutput().getMagnitude (2, 0, 256), 0.1f) << "PFL carries pre-fader audio";

    engine.getMixer().setFader (0, 1.0f);
    dev.renderBlocks (4);
    EXPECT_GT (dev.getLastOutput().getMagnitude (0, 0, 256), 0.1f);
    EXPECT_GT (engine.getMixer().getChannelPeak (0), 0.1f);
}

TEST_F (DjEngineSlow, WorksOnATwoChannelAndAOneChannelDevice)
{
    for (int outs : { 1, 2 })
    {
        SCOPED_TRACE ("outs=" + std::to_string (outs));
        DjEngine engine;
        MockAudioDevice dev (kRate, 256, 0, outs);
        engine.getDeck (1).load (file);
        dev.start (engine);
        engine.getDeck (1).play();
        ASSERT_TRUE (pumpUntil (dev, [&] { return engine.getDeck (1).getPositionFrames() > 2000; }));
        dev.renderBlocks (2);
        EXPECT_GT (dev.getLastOutput().getMagnitude (0, 0, 256), 0.1f);
    }
}

TEST_F (DjEngineTest, OversizeBlocksAreChunked)
{
    DjEngine engine;
    engine.prepare (kRate, 64);
    juce::AudioBuffer<float> out (2, 300);
    float* ptrs[2] { out.getWritePointer (0), out.getWritePointer (1) };
    engine.process (nullptr, 0, ptrs, 2, 300);
    EXPECT_EQ (out.getMagnitude (0, 300), 0.0f);
}
