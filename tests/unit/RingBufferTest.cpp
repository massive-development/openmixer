#include <JuceHeader.h>
#include "engine/RingBuffer.h"

#include <atomic>
#include <thread>
#include <vector>

namespace OpenMixerTests
{
    class RingBufferTest : public juce::UnitTest
    {
    public:
        RingBufferTest() : juce::UnitTest ("SPSC Ring Buffer", "OpenMixer Engine") {}

        void runTest() override
        {
            using openmixer::SpscRingBuffer;

            beginTest ("Capacity rounds up to power of two");
            {
                SpscRingBuffer<float> rb (100);
                expectEquals ((int) rb.getCapacity(), 128);
                expectEquals ((int) rb.getWritableCount(), 128);
                expectEquals ((int) rb.getReadableCount(), 0);
            }

            beginTest ("Basic write/read");
            {
                SpscRingBuffer<int> rb (8);
                int in[5] = { 1, 2, 3, 4, 5 }, out[5] = {};
                expectEquals ((int) rb.write (in, 5), 5);
                expectEquals ((int) rb.getReadableCount(), 5);
                expectEquals ((int) rb.read (out, 5), 5);
                for (int i = 0; i < 5; ++i) expectEquals (out[i], in[i]);
            }

            beginTest ("Overflow is truncated, not overwritten");
            {
                SpscRingBuffer<int> rb (8);
                int in[12];
                for (int i = 0; i < 12; ++i) in[i] = i;
                expectEquals ((int) rb.write (in, 12), 8);
                expectEquals ((int) rb.write (in, 1), 0);
                int out[8];
                expectEquals ((int) rb.read (out, 8), 8);
                for (int i = 0; i < 8; ++i) expectEquals (out[i], i);
            }

            beginTest ("Underrun returns fewer items");
            {
                SpscRingBuffer<int> rb (8);
                int in[3] = { 7, 8, 9 }, out[8] = {};
                rb.write (in, 3);
                expectEquals ((int) rb.read (out, 8), 3);
                expectEquals ((int) rb.read (out, 8), 0);
            }

            beginTest ("Wraparound preserves order");
            {
                SpscRingBuffer<int> rb (8);
                int counter = 0, expected = 0;
                for (int round = 0; round < 100; ++round)
                {
                    int in[5], out[5];
                    for (auto& v : in) v = counter++;
                    expectEquals ((int) rb.write (in, 5), 5);
                    expectEquals ((int) rb.read (out, 5), 5);
                    for (auto v : out) expectEquals (v, expected++);
                }
            }

            beginTest ("Skip discards items");
            {
                SpscRingBuffer<int> rb (8);
                int in[6] = { 0, 1, 2, 3, 4, 5 }, out[2];
                rb.write (in, 6);
                expectEquals ((int) rb.skip (4), 4);
                expectEquals ((int) rb.read (out, 2), 2);
                expectEquals (out[0], 4);
                expectEquals (out[1], 5);
                expectEquals ((int) rb.skip (10), 0);
            }

            beginTest ("Threaded producer/consumer delivers every item in order");
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
                        const int w = (int) rb.write (block, (size_t) n);
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
                        const int r = (int) rb.read (block, 53);
                        for (int i = 0; i < r; ++i)
                            if (block[i] != expected++) { failed = true; return; }
                        if (r == 0) std::this_thread::yield();
                    }
                });

                producer.join();
                consumer.join();
                expect (! failed.load(), "items must arrive in order without loss or duplication");
                expectEquals ((int) rb.getReadableCount(), 0);
            }
        }
    };

    RingBufferTest ringBufferTest;
}
