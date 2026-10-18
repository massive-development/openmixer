// OpenMixer OutputRouterTest - master/cue pair routing onto the device outputs
#include <JuceHeader.h>
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
