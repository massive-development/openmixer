// OpenMixer RingBufferTest - SPSC ring buffer behaviour
#include <JuceHeader.h>
#include "engine/RingBuffer.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <atomic>
#include <thread>

using openmixer::SpscRingBuffer;

TEST (RingBufferTest, CapacityRoundsUpToPowerOfTwo)
{
    SpscRingBuffer<float> rb (100);
    EXPECT_EQ (static_cast<int> (rb.getCapacity()), 128);
    EXPECT_EQ (static_cast<int> (rb.getWritableCount()), 128);
    EXPECT_EQ (static_cast<int> (rb.getReadableCount()), 0);
}

TEST (RingBufferTest, BasicWriteRead)
{
    SpscRingBuffer<int> rb (8);
    int in[5] = { 1, 2, 3, 4, 5 }, out[5] = {};
    EXPECT_EQ (static_cast<int> (rb.write (in, 5)), 5);
    EXPECT_EQ (static_cast<int> (rb.getReadableCount()), 5);
    EXPECT_EQ (static_cast<int> (rb.read (out, 5)), 5);
    for (int i = 0; i < 5; ++i)
        EXPECT_EQ (out[i], in[i]);
}

TEST (RingBufferTest, OverflowIsTruncatedNotOverwritten)
{
    SpscRingBuffer<int> rb (8);
    int in[12];
    for (int i = 0; i < 12; ++i) in[i] = i;
    EXPECT_EQ (static_cast<int> (rb.write (in, 12)), 8);
    EXPECT_EQ (static_cast<int> (rb.write (in, 1)), 0);
    int out[8];
    EXPECT_EQ (static_cast<int> (rb.read (out, 8)), 8);
    for (int i = 0; i < 8; ++i)
        EXPECT_EQ (out[i], i);
}

TEST (RingBufferTest, UnderrunReturnsFewerItems)
{
    SpscRingBuffer<int> rb (8);
    int in[3] = { 7, 8, 9 }, out[8] = {};
    rb.write (in, 3);
    EXPECT_EQ (static_cast<int> (rb.read (out, 8)), 3);
    EXPECT_EQ (static_cast<int> (rb.read (out, 8)), 0);
}

TEST (RingBufferTest, WraparoundPreservesOrder)
{
    SpscRingBuffer<int> rb (8);
    int counter = 0, expected = 0;
    for (int round = 0; round < 100; ++round)
    {
        int in[5], out[5];
        for (auto& v : in) v = counter++;
        ASSERT_EQ (static_cast<int> (rb.write (in, 5)), 5);
        ASSERT_EQ (static_cast<int> (rb.read (out, 5)), 5);
        for (auto v : out)
            EXPECT_EQ (v, expected++);
    }
}

TEST (RingBufferTest, SkipDiscardsItems)
{
    SpscRingBuffer<int> rb (8);
    int in[6] = { 0, 1, 2, 3, 4, 5 }, out[2];
    rb.write (in, 6);
    EXPECT_EQ (static_cast<int> (rb.skip (4)), 4);
    EXPECT_EQ (static_cast<int> (rb.read (out, 2)), 2);
    EXPECT_EQ (out[0], 4);
    EXPECT_EQ (out[1], 5);
    EXPECT_EQ (static_cast<int> (rb.skip (10)), 0);
}

TEST (RingBufferSlow, ThreadedProducerConsumerDeliversEveryItemInOrder)
{
    constexpr int total = 2000000;
    SpscRingBuffer<int> rb (1024);
    std::atomic<bool> failed { false };

    std::thread producer ([&]
    {
        int next = 0;
        int block[37];
        while (next < total)
        {
            const int n = std::min (37, total - next);
            for (int i = 0; i < n; ++i) block[i] = next + i;
            const int w = static_cast<int> (rb.write (block, static_cast<size_t> (n)));
            next += w;
            if (w == 0) std::this_thread::yield();
        }
    });

    std::thread consumer ([&]
    {
        int expected = 0;
        int block[53];
        while (expected < total)
        {
            const int r = static_cast<int> (rb.read (block, 53));
            for (int i = 0; i < r; ++i)
                if (block[i] != expected++) { failed = true; return; }
            if (r == 0) std::this_thread::yield();
        }
    });

    producer.join();
    consumer.join();
    EXPECT_FALSE (failed.load()) << "items must arrive in order without loss or duplication";
    EXPECT_EQ (static_cast<int> (rb.getReadableCount()), 0);
}
