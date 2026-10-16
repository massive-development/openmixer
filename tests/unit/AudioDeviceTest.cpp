#include <JuceHeader.h>
#include "engine/AudioDevice.h"

namespace OpenMixerTests
{
    namespace
    {
        /** Copies input to output with gain, counting calls and lifecycle events. */
        struct GainCallback : openmixer::AudioCallback
        {
            float gain = 0.5f;
            int prepared = 0, released = 0, calls = 0;
            double sr = 0; int block = 0;
            int sleepMs = 0;

            void prepare (double s, int b) override { sr = s; block = b; ++prepared; }
            void release() override { ++released; }
            void process (const float* const* in, int nIn, float* const* out, int nOut, int n) override
            {
                ++calls;
                if (sleepMs > 0) juce::Thread::sleep (sleepMs);
                for (int c = 0; c < nOut; ++c)
                    for (int i = 0; i < n; ++i)
                        out[c][i] = c < nIn ? in[c][i] * gain : 0.0f;
            }
        };
    }

    class AudioDeviceTest : public juce::UnitTest
    {
    public:
        AudioDeviceTest() : juce::UnitTest ("Audio Device", "OpenMixer Engine") {}

        void runTest() override
        {
            using namespace openmixer;

            beginTest ("Mock device lifecycle and block rendering");
            {
                MockAudioDevice dev (44100.0, 128, 2, 2);
                GainCallback cb;
                expect (! dev.isRunning());
                expect (! dev.renderBlock(), "cannot render before start");
                expect (dev.start (cb));
                expect (! dev.start (cb), "double start rejected");
                expectEquals (cb.prepared, 1);
                expectEquals (cb.sr, 44100.0);
                expectEquals (cb.block, 128);

                for (int c = 0; c < 2; ++c)
                    for (int i = 0; i < 128; ++i)
                        dev.getInputBuffer().setSample (c, i, c == 0 ? 1.0f : -1.0f);

                expectEquals (dev.renderBlocks (4), 4);
                expectEquals (cb.calls, 4);
                expectWithinAbsoluteError (dev.getLastOutput().getSample (0, 10), 0.5f, 1e-6f);
                expectWithinAbsoluteError (dev.getLastOutput().getSample (1, 10), -0.5f, 1e-6f);

                dev.stop();
                expectEquals (cb.released, 1);
                expect (! dev.isRunning());
                expect (! dev.renderBlock());
            }

            beginTest ("Capture records output across blocks");
            {
                MockAudioDevice dev (48000.0, 64, 1, 1);
                GainCallback cb;
                cb.gain = 1.0f;
                dev.start (cb);
                dev.startCapture (200);
                for (int i = 0; i < 64; ++i) dev.getInputBuffer().setSample (0, i, 0.25f);
                dev.renderBlocks (10);               // 640 frames offered, capture holds 200
                expectEquals (dev.getCapturedFrames(), 200);
                expectWithinAbsoluteError (dev.getCapture().getSample (0, 199), 0.25f, 1e-6f);
            }

            beginTest ("Latency instrumentation: device latency and timing");
            {
                MockAudioDevice dev (48000.0, 480, 2, 2);
                dev.setSimulatedLatency (96, 240);
                GainCallback cb;
                dev.start (cb);
                dev.renderBlocks (20);
                auto s = dev.getLatencyStats();
                expectEquals ((int) s.callbackCount, 20);
                expectEquals (s.inputLatencySamples, 96);
                expectEquals (s.outputLatencySamples, 240);
                expectWithinAbsoluteError (s.inputLatencyMs, 2.0, 1e-9);
                expectWithinAbsoluteError (s.outputLatencyMs, 5.0, 1e-9);
                expectWithinAbsoluteError (s.roundTripLatencyMs, 1000.0 * (480 + 96 + 240) / 48000.0, 1e-9);
                expectWithinAbsoluteError (s.budgetUs, 10000.0, 1e-6);
                expect (s.maxCallbackUs >= s.avgCallbackUs);
                expect (s.avgCallbackUs >= 0.0);
                expectEquals ((int) s.xrunCount, 0);
            }

            beginTest ("Slow callbacks are counted as xruns");
            {
                MockAudioDevice dev (48000.0, 48, 2, 2);   // 1 ms budget
                GainCallback cb;
                cb.sleepMs = 5;
                dev.start (cb);
                dev.renderBlocks (3);
                auto s = dev.getLatencyStats();
                expectEquals ((int) s.xrunCount, 3);
                expect (s.maxCallbackUs >= 4000.0, "max callback time reflects the sleep");
                expect (s.cpuLoad > 1.0, "load above 100% of budget");
            }

            beginTest ("LatencyMonitor records synthetic timings");
            {
                LatencyMonitor m;
                m.configure (48000.0, 480);
                m.recordCallback (2000.0, 480);
                m.recordCallback (4000.0, 480);
                m.recordCallback (12000.0, 480);  // exceeds 10 ms budget
                auto s = m.getStats();
                expectEquals ((int) s.callbackCount, 3);
                expectEquals ((int) s.xrunCount, 1);
                expectWithinAbsoluteError (s.avgCallbackUs, 6000.0, 1e-6);
                expectWithinAbsoluteError (s.maxCallbackUs, 12000.0, 1e-6);
                expectWithinAbsoluteError (s.lastCallbackUs, 12000.0, 1e-6);
                m.reset();
                expectEquals ((int) m.getStats().callbackCount, 0);
            }

            beginTest ("Paced realtime thread drives the callback");
            {
                MockAudioDevice dev (48000.0, 480, 2, 2);   // 10 ms blocks
                GainCallback cb;
                dev.start (cb);
                dev.startRealtime();
                juce::Thread::sleep (120);
                dev.stop();
                const int calls = cb.calls;
                expect (calls >= 5 && calls <= 20, "expected roughly 12 blocks, got " + juce::String (calls));
                juce::Thread::sleep (30);
                expectEquals (cb.calls, calls);   // no callbacks after stop
            }
        }
    };

    AudioDeviceTest audioDeviceTest;
}
