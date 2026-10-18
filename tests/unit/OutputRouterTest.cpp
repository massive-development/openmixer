// OpenMixer OutputRouterTest - master/cue pair routing onto the device outputs
#include <JuceHeader.h>
#include "LibraryIngestTestUtil.h"
#include "engine/DjEngine.h"
#include "engine/OutputRouter.h"

#include <gtest/gtest.h>

#include <array>
#include <vector>

using namespace openmixer;

namespace
{
// Engine stand-in: output channel c carries the constant (c + 1) so routing is easy to read.
struct ConstantSource final : AudioCallback
{
    void process (const float* const*, int, float* const* outputs, int numOutputs, int numSamples) override
    {
        for (int c = 0; c < numOutputs; ++c)
            for (int i = 0; i < numSamples; ++i)
                outputs[c][i] = static_cast<float> (c + 1);
        ++blocks;
        EXPECT_EQ (numOutputs, 4);
    }
    int blocks = 0;
};

struct Device
{
    explicit Device (int channels, int samples) : buffers (static_cast<size_t> (channels), std::vector<float> (static_cast<size_t> (samples), -9.0F))
    {
        for (auto& b : buffers)
            ptrs.push_back (b.data());
    }
    std::vector<std::vector<float>> buffers;
    std::vector<float*> ptrs;
    float at (int channel, int i = 0) const { return buffers[static_cast<size_t> (channel)][static_cast<size_t> (i)]; }
};
}

TEST (OutputRouterTest, DefaultsSendMasterToPairZeroAndCueToPairOne)
{
    OutputRouter router;
    router.prepare (64);
    ConstantSource src;
    Device dev (4, 64);
    router.process (src, dev.ptrs.data(), 4, 64);
    EXPECT_EQ (dev.at (0), 1.0F); EXPECT_EQ (dev.at (1), 2.0F);
    EXPECT_EQ (dev.at (2), 3.0F); EXPECT_EQ (dev.at (3), 4.0F);
}

TEST (OutputRouterTest, PairsCanBeSwappedAndMovedToHigherOutputs)
{
    OutputRouter router;
    router.prepare (64);
    router.setMasterPair (2);
    router.setCuePair (0);
    ConstantSource src;
    Device dev (6, 64);
    router.process (src, dev.ptrs.data(), 6, 64);
    EXPECT_EQ (dev.at (0), 3.0F); EXPECT_EQ (dev.at (1), 4.0F);   // cue on 1+2
    EXPECT_EQ (dev.at (2), 0.0F); EXPECT_EQ (dev.at (3), 0.0F);   // unrouted pair silenced
    EXPECT_EQ (dev.at (4), 1.0F); EXPECT_EQ (dev.at (5), 2.0F);   // master on 5+6
}

TEST (OutputRouterTest, NoCuePairLeavesOnlyTheMaster)
{
    OutputRouter router;
    router.prepare (32);
    router.setCuePair (OutputRouter::kNoPair);
    ConstantSource src;
    Device dev (4, 32);
    router.process (src, dev.ptrs.data(), 4, 32);
    EXPECT_EQ (dev.at (0), 1.0F);
    EXPECT_EQ (dev.at (2), 0.0F);
    EXPECT_EQ (dev.at (3), 0.0F);
}

TEST (OutputRouterTest, OutOfRangePairsAreIgnoredWithoutWritingPastTheDevice)
{
    OutputRouter router;
    router.prepare (32);
    router.setMasterPair (7);
    router.setCuePair (1);   // a 2-channel device has no pair 1
    ConstantSource src;
    Device dev (2, 32);
    router.process (src, dev.ptrs.data(), 2, 32);
    EXPECT_EQ (dev.at (0), 0.0F);
    EXPECT_EQ (dev.at (1), 0.0F);
}

TEST (OutputRouterTest, SameMasterAndCuePairKeepsTheMaster)
{
    OutputRouter router;
    router.prepare (16);
    router.setCuePair (0);
    ConstantSource src;
    Device dev (2, 16);
    router.process (src, dev.ptrs.data(), 2, 16);
    EXPECT_EQ (dev.at (0), 1.0F);
    EXPECT_EQ (dev.at (1), 2.0F);
}

TEST (OutputRouterTest, LongDeviceBlocksAreRenderedInChunks)
{
    OutputRouter router;
    router.prepare (16);
    ConstantSource src;
    Device dev (2, 50);
    router.process (src, dev.ptrs.data(), 2, 50);
    EXPECT_EQ (src.blocks, 4);   // 16 + 16 + 16 + 2
    for (int i = 0; i < 50; ++i)
        EXPECT_EQ (dev.at (0, i), 1.0F) << i;
}

TEST (OutputRouterTest, MonoDevicesGoStraightToTheSource)
{
    struct Probe final : AudioCallback
    {
        void process (const float* const*, int, float* const*, int numOutputs, int) override { seen = numOutputs; }
        int seen = 0;
    } probe;
    OutputRouter router;
    router.prepare (16);
    Device dev (1, 16);
    router.process (probe, dev.ptrs.data(), 1, 16);
    EXPECT_EQ (probe.seen, 1);
}

TEST (OutputRouterTest, ProcessBeforePrepareIsSilentAndTerminates)
{
    OutputRouter router;   // no prepare(): scratch has no room
    ConstantSource src;
    Device dev (2, 32);
    router.process (src, dev.ptrs.data(), 2, 32);
    EXPECT_EQ (src.blocks, 0);
    EXPECT_EQ (dev.at (0), 0.0F);
    EXPECT_EQ (dev.at (1, 31), 0.0F);
}

namespace
{
// Forwards to the real engine and keeps what it rendered, so each router block can be compared with the engine's own output.
struct CapturingEngine final : AudioCallback
{
    explicit CapturingEngine (DjEngine& e) : engine (e), capture (4, 4096) {}
    void process (const float* const* in, int numIn, float* const* out, int numOut, int n) override
    {
        engine.process (in, numIn, out, numOut, n);
        for (int c = 0; c < numOut; ++c)
            capture.copyFrom (c, 0, out[c], n);
        captured = n;
    }
    DjEngine& engine;
    juce::AudioBuffer<float> capture;
    int captured = 0;
};

bool waitUntil (const std::function<bool()>& cond)
{
    const auto end = juce::Time::getMillisecondCounter() + 20000;   // valgrind runs slowly
    while (! cond())
    {
        if (juce::Time::getMillisecondCounter() > end)
            return false;
        std::this_thread::yield();
    }
    return true;
}

juce::File writeTone (const juce::File& f)
{
    constexpr double rate = 44100.0;
    constexpr int frames = 44100 * 6;
    juce::AudioBuffer<float> buf (2, frames);
    for (int i = 0; i < frames; ++i)
        for (int c = 0; c < 2; ++c)
            buf.setSample (c, i, 0.3F * std::sin (2.0F * juce::MathConstants<float>::pi * 220.0F * static_cast<float> (i) / static_cast<float> (rate)));
    juce::WavAudioFormat wav;
    std::unique_ptr<juce::FileOutputStream> stream (f.createOutputStream());
    std::unique_ptr<juce::AudioFormatWriter> writer (wav.createWriterFor (stream.get(), rate, 2, 24, {}, 0));
    stream.release();
    writer->writeFromAudioSampleBuffer (buf, 0, frames);
    return f;
}

// Real engine playing a tone; every block the router's device output must equal the engine's own master/cue output.
void expectRouterMatchesEngine (int deviceChannels)
{
    constexpr int kBlock = 256;
    library::test::TempDir dir;
    DjEngine engine;
    ASSERT_TRUE (engine.getDeck (0).load (writeTone (dir.dir.getChildFile ("tone.wav"))));
    engine.prepare (44100.0, kBlock);
    float l[64], r[64];
    ASSERT_TRUE (waitUntil ([&] { engine.getDeck (0).renderStereo (l, r, 64); return engine.getDeck (0).getBufferedFrames() > 8192; }));
    engine.getDeck (0).play();

    OutputRouter router;
    router.prepare (kBlock);
    CapturingEngine source (engine);
    Device dev (deviceChannels, kBlock);
    float peak = 0.0F;
    for (int b = 0; b < 40; ++b)
    {
        waitUntil ([&] { return engine.getDeck (0).getBufferedFrames() >= 2048; });
        router.process (source, dev.ptrs.data(), deviceChannels, kBlock);
        ASSERT_EQ (source.captured, kBlock);
        for (int i = 0; i < kBlock; ++i)
        {
            for (int c = 0; c < 2; ++c)
                ASSERT_EQ (dev.at (c, i), source.capture.getSample (c, i)) << "master ch" << c << " block " << b;
            for (int c = 2; c < deviceChannels; ++c)
                ASSERT_EQ (dev.at (c, i), source.capture.getSample (c, i)) << "cue ch" << c << " block " << b;
            peak = std::max (peak, std::abs (dev.at (0, i)));
        }
    }
    EXPECT_GT (peak, 0.01F) << "the comparison must run on real audio";
}
}

TEST (OutputRouterTest, MatchesTheEngineDirectlyOnATwoChannelDevice) { expectRouterMatchesEngine (2); }
TEST (OutputRouterTest, MatchesTheEngineDirectlyOnAFourChannelDevice) { expectRouterMatchesEngine (4); }
