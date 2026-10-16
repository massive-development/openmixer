// OpenMixer AudioDeviceTest - mock audio device, latency stats and paced realtime thread
#include <JuceHeader.h>
#include "engine/MockAudioDevice.h"

#include <gtest/gtest.h>

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <mutex>

using namespace openmixer;

namespace
{
/** Copies input to output with gain, counting calls and lifecycle events. */
struct GainCallback : AudioCallback
{
    float gain = 0.5f;
    int prepared = 0, released = 0;
    std::atomic<int> calls { 0 };
    double sr = 0;
    int block = 0;
    int sleepMs = 0;   // simulates callback workload, not synchronisation

    std::mutex mutex;
    std::condition_variable cv;

    void prepare (double s, int b) override { sr = s; block = b; ++prepared; }
    void release() override { ++released; }
    void process (const float* const* in, int nIn, float* const* out, int nOut, int n) override
    {
        if (sleepMs > 0) juce::Thread::sleep (sleepMs);
        for (int c = 0; c < nOut; ++c)
            for (int i = 0; i < n; ++i)
                out[c][i] = c < nIn ? in[c][i] * gain : 0.0f;
        {
            std::lock_guard<std::mutex> lock (mutex);
            ++calls;
        }
        cv.notify_all();
    }

    bool waitForCalls (int count, std::chrono::milliseconds timeout)
    {
        std::unique_lock<std::mutex> lock (mutex);
        return cv.wait_for (lock, timeout, [&] { return calls.load() >= count; });
    }
};
}

TEST (AudioDeviceTest, MockDeviceLifecycleAndBlockRendering)
{
    MockAudioDevice dev (44100.0, 128, 2, 2);
    GainCallback cb;
    EXPECT_FALSE (dev.isRunning());
    EXPECT_FALSE (dev.renderBlock()) << "cannot render before start";
    ASSERT_TRUE (dev.start (cb));
    EXPECT_FALSE (dev.start (cb)) << "double start rejected";
    EXPECT_EQ (cb.prepared, 1);
    EXPECT_EQ (cb.sr, 44100.0);
    EXPECT_EQ (cb.block, 128);

    for (int c = 0; c < 2; ++c)
        for (int i = 0; i < 128; ++i)
            dev.getInputBuffer().setSample (c, i, c == 0 ? 1.0f : -1.0f);

    EXPECT_EQ (dev.renderBlocks (4), 4);
    EXPECT_EQ (cb.calls.load(), 4);
    EXPECT_NEAR (dev.getLastOutput().getSample (0, 10), 0.5f, 1e-6f);
    EXPECT_NEAR (dev.getLastOutput().getSample (1, 10), -0.5f, 1e-6f);

    dev.stop();
    EXPECT_EQ (cb.released, 1);
    EXPECT_FALSE (dev.isRunning());
    EXPECT_FALSE (dev.renderBlock());
}

TEST (AudioDeviceTest, CaptureRecordsOutputAcrossBlocks)
{
    MockAudioDevice dev (48000.0, 64, 1, 1);
    GainCallback cb;
    cb.gain = 1.0f;
    dev.start (cb);
    dev.startCapture (200);
    for (int i = 0; i < 64; ++i) dev.getInputBuffer().setSample (0, i, 0.25f);
    dev.renderBlocks (10);   // 640 frames offered, capture holds 200
    EXPECT_EQ (dev.getCapturedFrames(), 200);
    EXPECT_NEAR (dev.getCapture().getSample (0, 199), 0.25f, 1e-6f);
}

TEST (AudioDeviceTest, LatencyInstrumentationReportsDeviceLatencyAndTiming)
{
    MockAudioDevice dev (48000.0, 480, 2, 2);
    dev.setSimulatedLatency (96, 240);
    GainCallback cb;
    dev.start (cb);
    dev.renderBlocks (20);
    const auto s = dev.getLatencyStats();
    EXPECT_EQ (static_cast<int> (s.callbackCount), 20);
    EXPECT_EQ (s.inputLatencySamples, 96);
    EXPECT_EQ (s.outputLatencySamples, 240);
    EXPECT_NEAR (s.inputLatencyMs, 2.0, 1e-9);
    EXPECT_NEAR (s.outputLatencyMs, 5.0, 1e-9);
    EXPECT_NEAR (s.roundTripLatencyMs, 1000.0 * (480 + 96 + 240) / 48000.0, 1e-9);
    EXPECT_NEAR (s.budgetUs, 10000.0, 1e-6);
    EXPECT_GE (s.maxCallbackUs, s.avgCallbackUs);
    EXPECT_GE (s.avgCallbackUs, 0.0);
    EXPECT_EQ (static_cast<int> (s.xrunCount), 0);
}

TEST (AudioDeviceTest, SlowCallbacksAreCountedAsXruns)
{
    MockAudioDevice dev (48000.0, 48, 2, 2);   // 1 ms budget
    GainCallback cb;
    cb.sleepMs = 5;
    dev.start (cb);
    dev.renderBlocks (3);
    const auto s = dev.getLatencyStats();
    EXPECT_EQ (static_cast<int> (s.xrunCount), 3);
    EXPECT_GE (s.maxCallbackUs, 4000.0) << "max callback time reflects the sleep";
    EXPECT_GT (s.cpuLoad, 1.0) << "load above 100% of budget";
}

TEST (AudioDeviceTest, LatencyMonitorRecordsSyntheticTimings)
{
    LatencyMonitor m;
    m.configure (48000.0, 480);
    m.recordCallback (2000.0, 480);
    m.recordCallback (4000.0, 480);
    m.recordCallback (12000.0, 480);   // exceeds 10 ms budget
    const auto s = m.getStats();
    EXPECT_EQ (static_cast<int> (s.callbackCount), 3);
    EXPECT_EQ (static_cast<int> (s.xrunCount), 1);
    EXPECT_NEAR (s.avgCallbackUs, 6000.0, 1e-6);
    EXPECT_NEAR (s.maxCallbackUs, 12000.0, 1e-6);
    EXPECT_NEAR (s.lastCallbackUs, 12000.0, 1e-6);
    m.reset();
    EXPECT_EQ (static_cast<int> (m.getStats().callbackCount), 0);
}

TEST (AudioDeviceSlow, PacedRealtimeThreadDrivesTheCallback)
{
    MockAudioDevice dev (48000.0, 480, 2, 2);   // 10 ms blocks
    GainCallback cb;
    dev.start (cb);
    dev.startRealtime();
    // No upper bound: slow CI overshoots and the pacer catches up.
    EXPECT_TRUE (cb.waitForCalls (3, std::chrono::seconds (5))) << "paced thread should run blocks";
    dev.stop();
    const int calls = cb.calls.load();
    EXPECT_GE (calls, 3);
    EXPECT_EQ (cb.calls.load(), calls) << "no callbacks after stop";
}
